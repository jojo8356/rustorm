//! Checked, forward-only PostgreSQL SQL migrations.
//!
//! Initial SQL generation covers the supported scalar and relation schema subset.
//! Planning via a disposable shadow database supports a limited additive SQL subset.
//! Scripts are trusted operator input and generated migrations must be reviewed.
use crate::checksum::sha256;
use crate::schema::{DefaultValue, Scalar, Schema};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Invalid(String),
    History(String),
    Locked,
    #[cfg(feature = "postgres")]
    Database(crate::postgres::Error),
    #[cfg(feature = "postgres")]
    Decode(crate::row::DecodeError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "migration filesystem error: {e}"),
            Self::Invalid(s) => write!(f, "invalid migration: {s}"),
            Self::History(s) => write!(f, "migration history conflict: {s}"),
            Self::Locked => f.write_str("another migration operation holds a lock"),
            #[cfg(feature = "postgres")]
            Self::Database(e) => write!(f, "migration database error: {e}"),
            #[cfg(feature = "postgres")]
            Self::Decode(e) => write!(f, "migration history decoding error: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
#[cfg(feature = "postgres")]
impl From<crate::postgres::Error> for Error {
    fn from(e: crate::postgres::Error) -> Self {
        Self::Database(e)
    }
}
#[cfg(feature = "postgres")]
impl From<crate::row::DecodeError> for Error {
    fn from(e: crate::row::DecodeError) -> Self {
        Self::Decode(e)
    }
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn literal(value: &str) -> String {
    format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

/// Generate the first PostgreSQL migration, rejecting names PostgreSQL would truncate.
pub fn initial_sql(schema: &Schema) -> Result<String, Error> {
    let mut result =
        String::from("-- Initial Rustorm migration. Review this SQL before deployment.\n");
    if schema.models.is_empty() {
        return Err(Error::Invalid("schema has no models".into()));
    }
    crate::schema::validate_primary_keys(schema).map_err(|e| Error::Invalid(e.to_string()))?;
    crate::schema::validate_indexes(schema).map_err(|e| Error::Invalid(e.to_string()))?;
    crate::schema::validate_relations(schema).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut tables = std::collections::HashSet::new();
    for model in &schema.models {
        if model.table.is_empty()
            || model.table.contains('\0')
            || model.table.len() > 63
            || model.table == "_rustorm_migrations"
            || !tables.insert(&model.table)
        {
            return Err(Error::Invalid(format!(
                "unsupported, duplicate or reserved table name {:?}",
                model.table
            )));
        }
        result.push_str(&format!("CREATE TABLE {} (\n", quote(&model.table)));
        let mut fields = Vec::new();
        let mut columns = std::collections::HashSet::new();
        for field in &model.fields {
            if field.column.is_empty()
                || field.column.contains('\0')
                || field.column.len() > 63
                || !columns.insert(&field.column)
                || (field.id && field.nullable)
            {
                return Err(Error::Invalid(format!(
                    "invalid column: {:?}",
                    field.column
                )));
            }
            let valid = match &field.default {
                None => true,
                Some(DefaultValue::Autoincrement) => {
                    field.id && matches!(field.ty, Scalar::Int | Scalar::BigInt)
                }
                Some(DefaultValue::String(v)) => field.ty == Scalar::String && !v.contains('\0'),
                Some(DefaultValue::Boolean(_)) => field.ty == Scalar::Boolean,
                Some(DefaultValue::Number(v)) => match field.ty {
                    Scalar::Int => v.parse::<i32>().is_ok(),
                    Scalar::BigInt => v.parse::<i64>().is_ok(),
                    Scalar::Float => v.parse::<f64>().is_ok_and(|f| f.is_finite()),
                    _ => false,
                },
            };
            if !valid {
                return Err(Error::Invalid(format!(
                    "invalid default for {:?}",
                    field.column
                )));
            }
            let ty = match field.ty {
                Scalar::Int => "INTEGER",
                Scalar::BigInt => "BIGINT",
                Scalar::Float => "DOUBLE PRECISION",
                Scalar::Boolean => "BOOLEAN",
                Scalar::String => "TEXT",
            };
            let mut decl = format!("    {} {ty}", quote(&field.column));
            if field.default == Some(DefaultValue::Autoincrement) {
                decl.push_str(" GENERATED BY DEFAULT AS IDENTITY");
            }
            if !field.nullable {
                decl.push_str(" NOT NULL");
            }
            if field.id {
                decl.push_str(" PRIMARY KEY");
            } else if field.unique {
                decl.push_str(" UNIQUE");
            }
            if let Some(default) = &field.default {
                match default {
                    DefaultValue::Autoincrement => {}
                    DefaultValue::String(v) => decl.push_str(&format!(" DEFAULT {}", literal(v))),
                    DefaultValue::Number(v) => decl.push_str(&format!(" DEFAULT {v}")),
                    DefaultValue::Boolean(v) => decl.push_str(if *v {
                        " DEFAULT TRUE"
                    } else {
                        " DEFAULT FALSE"
                    }),
                }
            }
            fields.push(decl);
        }
        if !model.primary_key.is_empty() {
            let columns = model
                .primary_key
                .iter()
                .map(|name| {
                    let field = model
                        .fields
                        .iter()
                        .find(|field| &field.name == name)
                        .expect("validated primary key");
                    quote(&field.column)
                })
                .collect::<Vec<_>>()
                .join(", ");
            fields.push(format!("    PRIMARY KEY ({columns})"));
        }
        result.push_str(&fields.join(",\n"));
        result.push_str("\n);\n\n");
    }
    // Implicit N–N table orientation depends only on logical model names.
    // Names are short, deterministic, and checked against all user tables/indexes.
    let mut internal_names = std::collections::HashSet::new();
    for a in &schema.models {
        for relation in a.relations.iter().filter(|r| r.list && a.name < r.target) {
            let b = schema
                .models
                .iter()
                .find(|m| m.name == relation.target)
                .ok_or_else(|| Error::Invalid("relation target missing".into()))?;
            if !crate::schema::inverse_relation(a, relation, b).is_some_and(|r| r.list) {
                continue;
            }
            let left = a
                .fields
                .iter()
                .find(|f| f.id)
                .ok_or_else(|| Error::Invalid("missing primary key".into()))?;
            let right = b
                .fields
                .iter()
                .find(|f| f.id)
                .ok_or_else(|| Error::Invalid("missing primary key".into()))?;
            let table = crate::schema::implicit_join_name(&a.name, &b.name);
            let pk = format!("{table}_pk");
            let idx = format!("{table}_b_idx");
            for name in [&table, &pk, &idx] {
                if tables.iter().any(|t| *t == name) || !internal_names.insert(name.clone()) {
                    return Err(Error::Invalid(
                        "implicit relation object name collision".into(),
                    ));
                }
            }
            let sql_type = |ty| match ty {
                Scalar::Int => "INTEGER",
                Scalar::BigInt => "BIGINT",
                Scalar::Boolean => "BOOLEAN",
                Scalar::String => "TEXT",
                Scalar::Float => unreachable!("validated relation"),
            };
            result.push_str(&format!("CREATE TABLE {} (\n    \"a\" {} NOT NULL,\n    \"b\" {} NOT NULL,\n    CONSTRAINT {} PRIMARY KEY (\"a\", \"b\"),\n    FOREIGN KEY (\"a\") REFERENCES {} ({}) ON DELETE CASCADE ON UPDATE CASCADE,\n    FOREIGN KEY (\"b\") REFERENCES {} ({}) ON DELETE CASCADE ON UPDATE CASCADE\n);\nCREATE INDEX {} ON {} (\"b\");\n\n", quote(&table), sql_type(left.ty), sql_type(right.ty), quote(&pk), quote(&a.table), quote(&left.column), quote(&b.table), quote(&right.column), quote(&idx), quote(&table)));
        }
    }
    // Foreign keys are emitted after every table, including targets declared later.
    for model in &schema.models {
        for relation in &model.relations {
            if let Some(fk) = &relation.foreign_key {
                let target = schema
                    .models
                    .iter()
                    .find(|m| m.name == relation.target)
                    .ok_or_else(|| Error::Invalid("relation target missing".into()))?;
                let local = model
                    .fields
                    .iter()
                    .find(|f| f.name == fk.field)
                    .ok_or_else(|| Error::Invalid("relation field missing".into()))?;
                let remote = target
                    .fields
                    .iter()
                    .find(|f| f.name == fk.references)
                    .ok_or_else(|| Error::Invalid("referenced field missing".into()))?;
                let digest = sha256(format!("{}\0{}", model.table, local.column).as_bytes());
                let name = format!("rustorm_fk_{}", &digest[..48]);
                result.push_str(&format!("ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON DELETE {} ON UPDATE {};\n", quote(&model.table), quote(&name), quote(&local.column), quote(&target.table), quote(&remote.column), fk.on_delete.sql(), fk.on_update.sql()));
            }
        }
    }
    let mut index_names = std::collections::HashSet::new();
    for model in &schema.models {
        for index in &model.indexes {
            let field = model
                .fields
                .iter()
                .find(|field| field.name == index.field)
                .ok_or_else(|| Error::Invalid("index field missing".into()))?;
            let digest = sha256(format!("{}\0{}", model.table, field.column).as_bytes());
            let name = format!("rustorm_idx_{}", &digest[..48]);
            if tables.iter().any(|table| **table == name)
                || internal_names.contains(&name)
                || !index_names.insert(name.clone())
            {
                return Err(Error::Invalid(
                    "index name collides with another object".into(),
                ));
            }
            result.push_str(&format!(
                "CREATE INDEX {} ON {} ({});\n",
                quote(&name),
                quote(&model.table),
                quote(&field.column)
            ));
        }
    }
    Ok(result)
}
#[derive(Debug, Clone)]
pub struct Migration {
    pub name: String,
    pub sql: String,
    pub checksum: String,
    pub statements: Vec<String>,
}
fn valid_dir_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn read_file(path: &Path) -> Result<String, Error> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES {
        return Err(Error::Invalid(format!(
            "migration file must be a regular file <= {MAX_BYTES} bytes: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid("migration file exceeds size limit".into()));
    }
    String::from_utf8(bytes).map_err(|_| Error::Invalid("migration file is not UTF-8".into()))
}
/// Load every migration in lexical order. Unknown files/links are rejected.
pub fn load(dir: &Path) -> Result<Vec<Migration>, Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::Invalid(
            "migration root must be a real directory".into(),
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Invalid("migration name is not UTF-8".into()))?;
        let meta = fs::symlink_metadata(entry.path())?;
        if !valid_dir_name(&name) || !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(Error::Invalid(format!(
                "unexpected migration entry {name:?}"
            )));
        }
        let mut files = fs::read_dir(entry.path())?;
        let file = files
            .next()
            .transpose()?
            .ok_or_else(|| Error::Invalid(format!("missing migration.sql in {name}")))?;
        if file.file_name() != "migration.sql" || files.next().transpose()?.is_some() {
            return Err(Error::Invalid(format!(
                "{name} must contain only migration.sql"
            )));
        }
        let sql = read_file(&file.path())?;
        let statements = split_sql(&sql)?;
        entries.push(Migration {
            name,
            checksum: sha256(sql.as_bytes()),
            sql,
            statements,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}
/// Cooperating writers serialize filesystem updates. The lock is a sibling so
/// readers fail closed only for actual partial migration directories.
struct FileLock(PathBuf);
impl FileLock {
    fn acquire(dir: &Path) -> Result<Self, Error> {
        let name = dir.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
            Error::Invalid("migration directory needs a valid final component".into())
        })?;
        let parent = dir.parent().unwrap_or(Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        fs::create_dir_all(parent)?;
        let path = parent.join(format!(".{name}.rustorm-write-lock"));
        match fs::create_dir(&path) {
            Ok(()) => Ok(Self(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(Error::Locked),
            Err(e) => Err(Error::Io(e)),
        }
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

/// Create initial schema SQL only when there are no previous migrations. Later
/// files are empty templates and must be edited before they can be deployed.
pub fn create(dir: &Path, name: &str, schema: &Schema) -> Result<PathBuf, Error> {
    if !valid_dir_name(name) || name.len() > 60 {
        return Err(Error::Invalid(
            "name must contain only ASCII letters, digits, '_' or '-' (max 60)".into(),
        ));
    }
    let _file_lock = FileLock::acquire(dir)?;
    let existing = load(dir)?;
    let contents = if existing.is_empty() {
        initial_sql(schema)?
    } else {
        "-- Add reviewed SQL statements here. Empty migrations cannot be deployed.\n".to_owned()
    };
    write_file(dir, name, &contents, &existing)
}

fn write_file(
    dir: &Path,
    name: &str,
    contents: &str,
    existing: &[Migration],
) -> Result<PathBuf, Error> {
    if !valid_dir_name(name) || name.len() > 60 {
        return Err(Error::Invalid(
            "name must contain only ASCII letters, digits, '_' or '-' (max 60)".into(),
        ));
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Invalid("system clock before Unix epoch".into()))?
        .as_nanos();
    let directory = format!("{ts:039}_{name}");
    if existing.last().is_some_and(|m| m.name >= directory) {
        return Err(Error::Invalid(
            "clock moved backwards; refusing out-of-order migration".into(),
        ));
    }
    fs::create_dir_all(dir)?;
    let root = fs::symlink_metadata(dir)?;
    if root.file_type().is_symlink() {
        return Err(Error::Invalid("migration root may not be a symlink".into()));
    }
    let path = dir.join(&directory);
    fs::create_dir(&path)?;
    let tmp = path.join(".migration.sql.tmp");
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path.join("migration.sql"))
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir(&path);
        return Err(e.into());
    }
    Ok(path)
}

/// Split PostgreSQL SQL without interpreting values. libpq's extended protocol
/// independently rejects multiple statements per execution. Transaction control,
/// COPY, session settings and procedural execution are deliberately forbidden.
pub fn split_sql(sql: &str) -> Result<Vec<String>, Error> {
    let b = sql.as_bytes();
    let (mut i, mut start) = (0, 0);
    let mut out = Vec::new();
    let mut words = Vec::<String>::new();
    let mut has_code = false;
    while i < b.len() {
        match b[i] {
            0 => return Err(Error::Invalid("SQL contains NUL".into())),
            c if c.is_ascii_whitespace() => {
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1usize;
                while i < b.len() && depth > 0 {
                    if b.get(i..i + 2) == Some(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b.get(i..i + 2) == Some(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err(Error::Invalid("unterminated SQL comment".into()));
                }
            }
            b'\'' | b'"' => {
                has_code = true;
                let quote = b[i];
                i += 1;
                let mut closed = false;
                while i < b.len() {
                    if b[i] == quote {
                        if b.get(i + 1) == Some(&quote) {
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else if quote == b'\'' && b[i] == b'\\' && i + 1 < b.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if !closed {
                    return Err(Error::Invalid("unterminated SQL quoted value".into()));
                }
            }
            b'$' => {
                let mut end = i + 1;
                if b.get(end) != Some(&b'$') {
                    if b.get(end)
                        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                    {
                        end += 1;
                        while b
                            .get(end)
                            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                        {
                            end += 1;
                        }
                    }
                }
                if b.get(end) == Some(&b'$') {
                    has_code = true;
                    let tag = &b[i..=end];
                    i = end + 1;
                    let mut closed = false;
                    while i + tag.len() <= b.len() {
                        if &b[i..i + tag.len()] == tag {
                            i += tag.len();
                            closed = true;
                            break;
                        }
                        i += 1;
                    }
                    if !closed {
                        return Err(Error::Invalid(
                            "unterminated SQL dollar-quoted value".into(),
                        ));
                    }
                } else {
                    has_code = true;
                    i += 1;
                }
            }
            b';' => {
                if has_code {
                    validate_statement(&words)?;
                    out.push(sql[start..i].trim().to_owned());
                }
                i += 1;
                start = i;
                words.clear();
                has_code = false;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                has_code = true;
                let begin = i;
                i += 1;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$')
                {
                    i += 1;
                }
                if words.len() < 4 {
                    words.push(sql[begin..i].to_ascii_uppercase());
                }
            }
            _ => {
                has_code = true;
                i += 1;
            }
        }
    }
    if has_code {
        validate_statement(&words)?;
        out.push(sql[start..].trim().to_owned());
    }
    Ok(out)
}
fn validate_statement(words: &[String]) -> Result<(), Error> {
    let first = words.first().map(String::as_str).unwrap_or("");
    if [
        "BEGIN",
        "START",
        "COMMIT",
        "END",
        "ROLLBACK",
        "ABORT",
        "SAVEPOINT",
        "RELEASE",
        "PREPARE",
        "EXECUTE",
        "DEALLOCATE",
        "SET",
        "RESET",
        "DISCARD",
        "COPY",
        "DO",
        "CALL",
    ]
    .contains(&first)
    {
        return Err(Error::Invalid(format!(
            "statement {first} is not allowed in a managed migration"
        )));
    }
    Ok(())
}

#[cfg(feature = "postgres")]
mod database {
    use super::*;
    use crate::postgres::{Connection, Transaction};
    use crate::query::{CompiledQuery, Value};
    use std::collections::{HashMap, HashSet};

    const LOCK_ID: i64 = 0x5255_5354_4f52_4d;
    const TABLE: &str = "\"public\".\"_rustorm_migrations\"";
    fn query(sql: &str, params: Vec<Value>) -> CompiledQuery {
        CompiledQuery {
            sql: sql.into(),
            params,
        }
    }
    fn lock_value(conn: &mut Connection, sql: &str) -> Result<bool, Error> {
        let rows = conn.query(&query(sql, vec![Value::BigInt(LOCK_ID)]))?;
        if rows.len() != 1 {
            return Err(Error::History(
                "lock returned an unexpected number of rows".into(),
            ));
        }
        Ok(rows[0].get::<bool>("locked")?)
    }
    struct Lock<'a> {
        conn: &'a mut Connection,
    }
    impl<'a> Lock<'a> {
        fn acquire(conn: &'a mut Connection) -> Result<Self, Error> {
            if !conn.transaction_is_idle() {
                return Err(Error::History(
                    "migration operations require an idle connection".into(),
                ));
            }
            if !lock_value(conn, "SELECT pg_try_advisory_lock($1) AS locked")? {
                return Err(Error::Locked);
            }
            Ok(Self { conn })
        }
    }
    impl Drop for Lock<'_> {
        fn drop(&mut self) {
            if !matches!(
                lock_value(self.conn, "SELECT pg_advisory_unlock($1) AS locked"),
                Ok(true)
            ) {
                self.conn.invalidate();
            }
        }
    }
    fn exists(conn: &mut Connection) -> Result<bool, Error> {
        let rows = conn.query(&query(
            "SELECT to_regclass('public._rustorm_migrations') IS NOT NULL AS present",
            vec![],
        ))?;
        if rows.len() != 1 {
            return Err(Error::History(
                "metadata check returned unexpected rows".into(),
            ));
        }
        Ok(rows[0].get("present")?)
    }
    fn validate_metadata(conn: &mut Connection) -> Result<(), Error> {
        let rows = conn.query(&query("SELECT c.relkind::text AS kind, obj_description(c.oid, 'pg_class')::text AS marker FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'public' AND c.relname = '_rustorm_migrations'", vec![]))?;
        if rows.len() != 1
            || rows[0].get::<String>("kind")? != "r"
            || rows[0].get::<Option<String>>("marker")?.as_deref()
                != Some("rustorm migration history v1")
        {
            return Err(Error::History(
                "existing migration metadata table is not a Rustorm v1 table".into(),
            ));
        }
        Ok(())
    }
    fn init(conn: &mut Connection) -> Result<(), Error> {
        if exists(conn)? {
            return validate_metadata(conn);
        }
        let mut tx = conn.begin()?;
        tx.execute_statement(&format!("CREATE TABLE {TABLE} (\n id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,\n name TEXT NOT NULL, checksum TEXT NOT NULL,\n status TEXT NOT NULL CHECK (status IN ('applying','applied','failed','rolled_back')),\n started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP, finished_at TIMESTAMPTZ,\n error_code TEXT\n)"))?;
        tx.execute_statement(&format!(
            "COMMENT ON TABLE {TABLE} IS 'rustorm migration history v1'"
        ))?;
        tx.commit()?;
        Ok(())
    }
    #[derive(Debug)]
    struct Entry {
        id: i64,
        name: String,
        checksum: String,
        status: String,
    }
    fn history(conn: &mut Connection) -> Result<Vec<Entry>, Error> {
        if !exists(conn)? {
            return Ok(Vec::new());
        }
        validate_metadata(conn)?;
        let rows = conn.query(&query(
            &format!("SELECT id, name, checksum, status FROM {TABLE} ORDER BY id"),
            vec![],
        ))?;
        rows.into_iter()
            .map(|r| {
                Ok(Entry {
                    id: r.get("id")?,
                    name: r.get("name")?,
                    checksum: r.get("checksum")?,
                    status: r.get("status")?,
                })
            })
            .collect()
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Status {
        pub applied: Vec<String>,
        pub pending: Vec<String>,
        pub failed: Vec<String>,
        pub problems: Vec<String>,
    }
    fn inspect(files: &[Migration], entries: &[Entry]) -> Status {
        let mut status = Status {
            applied: vec![],
            pending: vec![],
            failed: vec![],
            problems: vec![],
        };
        let by_name: HashMap<_, _> = files.iter().map(|f| (f.name.as_str(), f)).collect();
        let mut applied = HashSet::new();
        let mut unresolved = HashSet::new();
        let mut ordered_applied = Vec::new();
        for e in entries {
            if e.status == "rolled_back" {
                continue;
            }
            if !["applying", "applied", "failed"].contains(&e.status.as_str()) {
                status
                    .problems
                    .push(format!("unknown history status for {}", e.name));
                continue;
            }
            match by_name.get(e.name.as_str()) {
                None => status.problems.push(format!(
                    "migration {} exists in database but not on disk",
                    e.name
                )),
                Some(file) if file.checksum != e.checksum => status
                    .problems
                    .push(format!("checksum changed for {}", e.name)),
                _ => {}
            }
            if e.status == "applied" {
                if !applied.insert(e.name.as_str()) {
                    status
                        .problems
                        .push(format!("duplicate applied migration {}", e.name));
                }
                ordered_applied.push(e.name.as_str());
            } else {
                if !unresolved.insert(e.name.as_str()) {
                    status
                        .problems
                        .push(format!("multiple unresolved attempts for {}", e.name));
                }
                status.failed.push(format!("{} ({})", e.name, e.status));
            }
        }
        for name in &unresolved {
            if applied.contains(name) {
                status
                    .problems
                    .push(format!("{} is both applied and unresolved", name));
            }
        }
        let expected: Vec<_> = files
            .iter()
            .take(ordered_applied.len())
            .map(|f| f.name.as_str())
            .collect();
        if expected != ordered_applied {
            status
                .problems
                .push("applied migrations are not a prefix of the ordered files".into());
        }
        for file in files {
            if applied.contains(file.name.as_str()) {
                status.applied.push(file.name.clone());
            } else {
                status.pending.push(file.name.clone());
            }
        }
        status
    }
    /// Inspect local files and database history without creating the metadata table.
    pub fn status(conn: &mut Connection, dir: &Path) -> Result<Status, Error> {
        let files = load(dir)?;
        let guard = Lock::acquire(conn)?;
        let entries = history(guard.conn)?;
        Ok(inspect(&files, &entries))
    }
    fn apply(tx: &mut Transaction<'_>, file: &Migration, id: i64) -> Result<(), Error> {
        tx.execute_statement("SET LOCAL search_path = public")?;
        tx.execute_statement("SET LOCAL standard_conforming_strings = on")?;
        for statement in &file.statements {
            tx.execute_statement(statement)?;
            if !tx.transaction_is_active() {
                return Err(Error::History(
                    "migration transaction ended unexpectedly; inspect the database".into(),
                ));
            }
        }
        let count = tx.execute(&query(&format!("UPDATE {TABLE} SET status = 'applied', finished_at = CURRENT_TIMESTAMP WHERE id = $1 AND status = 'applying'"), vec![Value::BigInt(id)]))?;
        if count != 1 {
            return Err(Error::History(
                "migration attempt disappeared during application".into(),
            ));
        }
        Ok(())
    }
    /// Mark the first migration as applied on an existing database without executing SQL.
    /// The caller must independently verify that its schema matches the migration.
    /// This operation is only allowed when there is no prior Rustorm history.
    pub fn baseline(conn: &mut Connection, dir: &Path, name: &str) -> Result<(), Error> {
        let files = load(dir)?;
        let first = files
            .first()
            .ok_or_else(|| Error::History("baseline requires a first migration".into()))?;
        if first.name != name {
            return Err(Error::History(
                "baseline must name the first migration".into(),
            ));
        }
        if first.statements.is_empty() {
            return Err(Error::Invalid("baseline migration is empty".into()));
        }
        let guard = Lock::acquire(conn)?;
        if !history(guard.conn)?.is_empty() {
            return Err(Error::History(
                "baseline requires an empty migration history".into(),
            ));
        }
        init(guard.conn)?;
        let count = guard.conn.execute(&query(&format!("INSERT INTO {TABLE} (name, checksum, status, finished_at) VALUES ($1, $2, 'applied', CURRENT_TIMESTAMP)"), vec![Value::String(first.name.clone()), Value::String(first.checksum.clone())]))?;
        if count != 1 {
            return Err(Error::History(
                "baseline could not be recorded; inspect history before retrying".into(),
            ));
        }
        Ok(())
    }
    /// Apply ordered files exactly once. Failed attempts block all further deployment.
    pub fn deploy(conn: &mut Connection, dir: &Path) -> Result<Vec<String>, Error> {
        let files = load(dir)?;
        for f in &files {
            if f.statements.is_empty() {
                return Err(Error::Invalid(format!("{} is empty", f.name)));
            }
        }
        let guard = Lock::acquire(conn)?;
        init(guard.conn)?;
        let entries = history(guard.conn)?;
        let report = inspect(&files, &entries);
        if !report.problems.is_empty() {
            return Err(Error::History(report.problems.join("; ")));
        }
        if !report.failed.is_empty() {
            return Err(Error::History(format!(
                "unresolved migration attempts: {}",
                report.failed.join(", ")
            )));
        }
        let mut applied_now = Vec::new();
        for file in files.iter().filter(|m| report.pending.contains(&m.name)) {
            let rows = guard.conn.query(&query(&format!("INSERT INTO {TABLE} (name, checksum, status) VALUES ($1, $2, 'applying') RETURNING id"), vec![Value::String(file.name.clone()), Value::String(file.checksum.clone())]))?;
            if rows.len() != 1 {
                return Err(Error::History("unable to record migration attempt".into()));
            }
            let id: i64 = rows[0].get("id")?;
            let result = (|| {
                let mut tx = guard.conn.begin()?;
                apply(&mut tx, file, id)?;
                tx.commit()?;
                Ok::<(), Error>(())
            })();
            if let Err(e) = result {
                // An uncertain commit must never be blindly retried. If the commit
                // succeeded the guarded UPDATE does not change the applied row.
                let mark = guard.conn.execute(&query(&format!("UPDATE {TABLE} SET status = 'failed', finished_at = CURRENT_TIMESTAMP, error_code = $2 WHERE id = $1 AND status = 'applying'"), vec![Value::BigInt(id), Value::String("execution_error".into())]));
                return match mark {
                    Ok(1) => Err(Error::History(format!(
                        "{} failed and must be resolved: {e}",
                        file.name
                    ))),
                    _ => Err(Error::History(format!(
                        "outcome of {} is uncertain; inspect database history before resolving",
                        file.name
                    ))),
                };
            }
            applied_now.push(file.name.clone());
        }
        Ok(applied_now)
    }
    include!("migration_plan.rs");

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Resolution {
        Applied,
        RolledBack,
    }
    /// Record an operator decision; this does not execute or undo SQL.
    pub fn resolve(
        conn: &mut Connection,
        dir: &Path,
        name: &str,
        resolution: Resolution,
    ) -> Result<(), Error> {
        let files = load(dir)?;
        let guard = Lock::acquire(conn)?;
        let entries = history(guard.conn)?;
        let report = inspect(&files, &entries);
        if !report.problems.is_empty() {
            return Err(Error::History(report.problems.join("; ")));
        }
        let active: Vec<_> = entries
            .iter()
            .filter(|e| e.status == "applying" || e.status == "failed")
            .collect();
        if active.len() != 1 || active[0].name != name {
            return Err(Error::History(
                "resolution requires exactly one matching unresolved attempt".into(),
            ));
        }
        if report.pending.first().map(String::as_str) != Some(name) {
            return Err(Error::History(
                "unresolved migration is not the next migration".into(),
            ));
        }
        let value = match resolution {
            Resolution::Applied => "applied",
            Resolution::RolledBack => "rolled_back",
        };
        let n = guard.conn.execute(&query(&format!("UPDATE {TABLE} SET status = $2, finished_at = CURRENT_TIMESTAMP WHERE id = $1 AND status IN ('applying','failed')"), vec![Value::BigInt(active[0].id), Value::String(value.into())]))?;
        if n != 1 {
            return Err(Error::History(
                "resolution did not update one attempt".into(),
            ));
        }
        Ok(())
    }
}
#[cfg(feature = "postgres")]
pub use database::{baseline, deploy, plan, resolve, status, write_plan, Plan, Resolution, Status};
