// Included inside migrations::database to share advisory locks and history checks.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogColumn {
    ty: String,
    not_null: bool,
    identity: String,
    identity_config: String,
    default: Option<String>,
    primary: bool,
    unique: bool,
}
type Catalog = BTreeMap<String, BTreeMap<String, CatalogColumn>>;

// This planner intentionally refuses features it cannot compare safely. Extra
// indexes, constraints, triggers, routines, custom types and non-identity sequences
// are not silently treated as equivalent to an .rsm schema.
const UNSUPPORTED_SQL: &str = r#"
SELECT (
 EXISTS (SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE n.nspname=$1 AND ($2::boolean = false OR c.relname <> '_rustorm_migrations')
   AND (c.relkind NOT IN ('r','i','S') OR (c.relkind='r' AND (c.relpersistence <> 'p' OR c.relispartition OR c.relrowsecurity OR c.relforcerowsecurity OR EXISTS (SELECT 1 FROM pg_catalog.pg_inherits h WHERE h.inhrelid=c.oid OR h.inhparent=c.oid))) OR (c.relkind='S' AND NOT EXISTS (
     SELECT 1 FROM pg_catalog.pg_depend d JOIN pg_catalog.pg_class owner ON owner.oid=d.refobjid
     WHERE d.classid='pg_catalog.pg_class'::regclass AND d.refclassid='pg_catalog.pg_class'::regclass
       AND d.objid=c.oid AND d.deptype='i' AND owner.relnamespace=c.relnamespace))))
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid
   JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE n.nspname=$1 AND ($2::boolean = false OR c.relname <> '_rustorm_migrations')
   AND (k.contype NOT IN ('p','u','n') OR cardinality(k.conkey) <> 1 OR k.condeferrable OR NOT k.convalidated))
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON c.oid=i.indrelid
   JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE n.nspname=$1 AND ($2::boolean = false OR c.relname <> '_rustorm_migrations')
   AND (NOT i.indisvalid OR NOT i.indisready OR i.indnullsnotdistinct OR i.indpred IS NOT NULL OR i.indexprs IS NOT NULL
     OR NOT EXISTS (SELECT 1 FROM pg_catalog.pg_constraint k WHERE k.conindid=i.indexrelid)))
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid
   JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace JOIN pg_catalog.pg_type t ON t.oid=a.atttypid
   WHERE n.nspname=$1 AND c.relkind='r' AND a.attnum > 0 AND NOT a.attisdropped
   AND ($2::boolean = false OR c.relname <> '_rustorm_migrations')
   AND (a.attgenerated <> '' OR a.attcollation <> t.typcollation))
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t JOIN pg_catalog.pg_class c ON c.oid=t.tgrelid
   JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE n.nspname=$1 AND ($2::boolean = false OR c.relname <> '_rustorm_migrations') AND NOT t.tgisinternal)
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname=$1)
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid=t.typnamespace
   WHERE n.nspname=$1 AND t.typtype IN ('e','d','r','m'))
) AS unsupported
"#;
const COLUMNS_SQL: &str = r#"
SELECT c.relname::text AS tablename, a.attname::text AS columnname,
 pg_catalog.format_type(a.atttypid,a.atttypmod)::text AS typename,
 a.attnotnull AS notnull, a.attidentity::text AS identitykind,
 pg_catalog.concat_ws('|', seq.seqstart, seq.seqincrement, seq.seqmin, seq.seqmax, seq.seqcache, seq.seqcycle)::text AS identityconfig,
 pg_catalog.pg_get_expr(d.adbin,d.adrelid)::text AS defaultvalue,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint k WHERE k.conrelid=c.oid AND k.contype='p' AND k.conkey=ARRAY[a.attnum]::smallint[]) AS isprimary,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint k WHERE k.conrelid=c.oid AND k.contype='u' AND k.conkey=ARRAY[a.attnum]::smallint[]) AS isunique
FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid AND a.attnum > 0 AND NOT a.attisdropped
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=c.oid AND d.adnum=a.attnum
LEFT JOIN (pg_catalog.pg_depend dep JOIN pg_catalog.pg_class seqclass ON seqclass.oid=dep.objid AND seqclass.relkind='S'
 JOIN pg_catalog.pg_sequence seq ON seq.seqrelid=seqclass.oid)
 ON dep.classid='pg_catalog.pg_class'::regclass AND dep.refclassid='pg_catalog.pg_class'::regclass
 AND dep.refobjid=c.oid AND dep.refobjsubid=a.attnum AND dep.deptype='i'
WHERE n.nspname=$1 AND c.relkind='r' AND ($2::boolean = false OR c.relname <> '_rustorm_migrations')
ORDER BY c.relname,a.attnum
"#;
const EMPTY_SQL: &str = r#"
SELECT (
 EXISTS (SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public')
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
 OR EXISTS (SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname NOT IN ('public','pg_catalog','information_schema') AND pg_catalog.left(nspname,3) <> 'pg_')
) AS occupied
"#;
fn catalog<F>(mut run: F, namespace: &str, skip_metadata: bool) -> Result<Catalog, Error>
where
    F: FnMut(&CompiledQuery) -> Result<Vec<crate::row::Row>, crate::postgres::Error>,
{
    let params = || {
        vec![
            Value::String(namespace.to_owned()),
            Value::Bool(skip_metadata),
        ]
    };
    let unsupported = run(&query(UNSUPPORTED_SQL, params()))?;
    if unsupported.len() != 1 || unsupported[0].get::<bool>("unsupported")? {
        return Err(Error::Invalid(format!("unsupported database objects in schema {namespace}; automatic planning is limited to simple scalar tables")));
    }
    let mut output: Catalog = BTreeMap::new();
    let tables = run(&query("SELECT c.relname::text AS tablename FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind='r' AND ($2::boolean=false OR c.relname <> '_rustorm_migrations') ORDER BY c.relname", params()))?;
    for row in tables {
        output.insert(row.get::<String>("tablename")?, BTreeMap::new());
    }
    for row in run(&query(COLUMNS_SQL, params()))? {
        let table: String = row.get("tablename")?;
        let column: String = row.get("columnname")?;
        let item = CatalogColumn {
            ty: row.get("typename")?,
            not_null: row.get("notnull")?,
            identity: row.get("identitykind")?,
            identity_config: row.get("identityconfig")?,
            default: row.get("defaultvalue")?,
            primary: row.get("isprimary")?,
            unique: row.get("isunique")?,
        };
        if output
            .entry(table)
            .or_default()
            .insert(column, item)
            .is_some()
        {
            return Err(Error::History("duplicate catalog column".into()));
        }
    }
    Ok(output)
}
fn db_name(conn: &mut Connection) -> Result<String, Error> {
    let rows = conn.query(&query("SELECT current_database()::text AS name", vec![]))?;
    if rows.len() != 1 {
        return Err(Error::History("cannot identify database".into()));
    }
    Ok(rows[0].get("name")?)
}
fn db_major_version(conn: &mut Connection) -> Result<i32, Error> {
    let rows = conn.query(&query(
        "SELECT current_setting('server_version_num')::integer AS version",
        vec![],
    ))?;
    if rows.len() != 1 {
        return Err(Error::History("cannot identify PostgreSQL version".into()));
    }
    let version: i32 = rows[0].get("version")?;
    if version < 150000 {
        return Err(Error::Invalid(
            "automatic planning requires PostgreSQL 15 or newer".into(),
        ));
    }
    Ok(version / 10000)
}
#[derive(PartialEq, Eq)]
struct DatabaseLocale {
    encoding: String,
    collate: String,
    ctype: String,
    version: Option<String>,
    provider: String,
}
fn db_locale(conn: &mut Connection) -> Result<DatabaseLocale, Error> {
    let rows = conn.query(&query("SELECT pg_catalog.pg_encoding_to_char(encoding)::text AS encoding, datcollate::text AS collate, datctype::text AS ctype, datcollversion::text AS version, datlocprovider::text AS provider FROM pg_catalog.pg_database WHERE datname=current_database()", vec![]))?;
    if rows.len() != 1 {
        return Err(Error::History("cannot inspect database locale".into()));
    }
    let locale = DatabaseLocale {
        encoding: rows[0].get("encoding")?,
        collate: rows[0].get("collate")?,
        ctype: rows[0].get("ctype")?,
        version: rows[0].get("version")?,
        provider: rows[0].get("provider")?,
    };
    if locale.provider != "c" {
        return Err(Error::Invalid(
            "automatic planning currently requires libc locale databases".into(),
        ));
    }
    Ok(locale)
}
fn replay(tx: &mut Transaction<'_>, files: &[Migration]) -> Result<(), Error> {
    tx.execute_statement("SET LOCAL search_path = public, pg_catalog")?;
    tx.execute_statement("SET LOCAL standard_conforming_strings = on")?;
    for file in files {
        if file.statements.is_empty() {
            return Err(Error::Invalid(format!("{} is empty", file.name)));
        }
        for statement in &file.statements {
            tx.execute_statement(statement)?;
            if !tx.transaction_is_active() {
                return Err(Error::History(
                    "shadow transaction ended unexpectedly".into(),
                ));
            }
        }
    }
    Ok(())
}
fn column_addition(field: &crate::schema::Field) -> Result<String, Error> {
    if field.id || field.unique || matches!(field.default, Some(DefaultValue::Autoincrement)) {
        return Err(Error::Invalid(format!(
            "adding a primary, unique or identity column {:?} requires a manual migration",
            field.column
        )));
    }
    if !field.nullable && field.default.is_none() {
        return Err(Error::Invalid(format!(
            "adding required column {:?} without a default requires a manual migration",
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
    let mut result = format!("{} {ty}", quote(&field.column));
    if !field.nullable {
        result.push_str(" NOT NULL");
    }
    match &field.default {
        None => {}
        Some(DefaultValue::String(v)) => result.push_str(&format!(" DEFAULT {}", literal(v))),
        Some(DefaultValue::Boolean(v)) => result.push_str(if *v {
            " DEFAULT TRUE"
        } else {
            " DEFAULT FALSE"
        }),
        Some(DefaultValue::Number(v)) => result.push_str(&format!(" DEFAULT {v}")),
        Some(DefaultValue::Autoincrement) => unreachable!(),
    }
    Ok(result)
}
/// Read-only migration plan. Its basis prevents writing after history files change.
#[derive(Debug, Clone)]
pub struct Plan {
    sql: String,
    basis: Vec<(String, String)>,
}
impl Plan {
    pub fn sql(&self) -> &str {
        &self.sql
    }
    pub fn is_empty(&self) -> bool {
        self.sql.is_empty()
    }
}
/// Replay trusted migrations in a transaction on an empty, separately named shadow
/// database and compute additive changes. The shadow transaction is rolled back.
pub fn plan(
    target: &mut Connection,
    shadow: &mut Connection,
    dir: &Path,
    schema: &Schema,
) -> Result<Plan, Error> {
    // The scalar catalog cannot compare foreign keys; never silently omit them.
    if schema
        .models
        .iter()
        .any(|model| !model.relations.is_empty())
    {
        return Err(Error::Invalid(
            "automatic planning does not yet support relations or foreign keys".into(),
        ));
    }
    if schema
        .models
        .iter()
        .any(|model| !model.primary_key.is_empty())
    {
        return Err(Error::Invalid(
            "automatic planning does not yet support composite primary keys".into(),
        ));
    }
    if schema.models.iter().any(|model| !model.indexes.is_empty()) {
        return Err(Error::Invalid(
            "automatic planning does not yet support custom indexes".into(),
        ));
    }
    // Validate all input before touching either database.
    let desired_sql = initial_sql(schema)?;
    let files = load(dir)?;
    if files.iter().any(|f| f.statements.is_empty()) {
        return Err(Error::Invalid("history contains an empty migration".into()));
    }
    let target_name = db_name(target)?;
    let shadow_name = db_name(shadow)?;
    if target_name == shadow_name {
        return Err(Error::Invalid(
            "shadow and target must have different database names".into(),
        ));
    }
    if db_major_version(target)? != db_major_version(shadow)? {
        return Err(Error::Invalid(
            "shadow and target must use the same PostgreSQL major version".into(),
        ));
    }
    if db_locale(target)? != db_locale(shadow)? {
        return Err(Error::Invalid(
            "shadow and target must use matching database encoding and locale".into(),
        ));
    }
    let target_guard = Lock::acquire(target)?;
    let shadow_guard = Lock::acquire(shadow)?;
    let report = inspect(&files, &history(target_guard.conn)?);
    if !report.problems.is_empty() || !report.failed.is_empty() || !report.pending.is_empty() {
        return Err(Error::History(
            "target history must be fully applied and free of conflicts before planning".into(),
        ));
    }
    let target_state = catalog(|q| target_guard.conn.query(q), "public", true)?;
    let occupied = shadow_guard.conn.query(&query(EMPTY_SQL, vec![]))?;
    if occupied.len() != 1 || occupied[0].get::<bool>("occupied")? {
        return Err(Error::Invalid(
            "shadow database must have no user objects in public or additional user schemas".into(),
        ));
    }
    let mut tx = shadow_guard.conn.begin()?;
    replay(&mut tx, &files)?;
    let extra = tx.query(&query("SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname NOT IN ('public','pg_catalog','information_schema') AND pg_catalog.left(nspname,3) <> 'pg_') AS extra", vec![]))?;
    if extra.len() != 1 || extra[0].get::<bool>("extra")? {
        return Err(Error::Invalid(
            "migration history creates unsupported schemas".into(),
        ));
    }
    let previous = catalog(|q| tx.query(q), "public", false)?;
    if target_state != previous {
        return Err(Error::History(
            "target schema differs from replayed migration history (supported scalar subset)"
                .into(),
        ));
    }
    tx.execute_statement("CREATE SCHEMA \"_rustorm_plan_expected\"")?;
    tx.execute_statement("SET LOCAL search_path = \"_rustorm_plan_expected\", pg_catalog")?;
    for statement in split_sql(&desired_sql)? {
        tx.execute_statement(&statement)?;
    }
    let desired = catalog(|q| tx.query(q), "_rustorm_plan_expected", false)?;
    let mut changes = Vec::new();
    for (table, old_columns) in &previous {
        let new_columns = desired.get(table).ok_or_else(|| {
            Error::Invalid(format!(
                "dropping or renaming table {table:?} requires a manual migration"
            ))
        })?;
        for (column, old) in old_columns {
            let new = new_columns.get(column).ok_or_else(|| {
                Error::Invalid(format!(
                    "dropping or renaming column {table}.{column} requires a manual migration"
                ))
            })?;
            if old != new {
                return Err(Error::Invalid(format!(
                    "changing column {table}.{column} requires a manual migration"
                )));
            }
        }
    }
    for model in &schema.models {
        if !previous.contains_key(&model.table) {
            let one = Schema {
                models: vec![model.clone()],
                generator_output: None,
            };
            changes.push(initial_sql(&one)?);
        } else {
            let old_columns = &previous[&model.table];
            for field in &model.fields {
                if !old_columns.contains_key(&field.column) {
                    changes.push(format!(
                        "ALTER TABLE {} ADD COLUMN {};\n",
                        quote(&model.table),
                        column_addition(field)?
                    ));
                }
            }
        }
    }
    let sql = if changes.is_empty() {
        String::new()
    } else {
        format!(
            "-- Generated additive migration. Review before deployment.\n{}",
            changes.join("\n")
        )
    };
    // Execute the proposal in the shadow and compare again before exposing it.
    // This catches SQL-generation mistakes and backend-specific behavior.
    if !sql.is_empty() {
        tx.execute_statement("SET LOCAL search_path = public, pg_catalog")?;
        for statement in split_sql(&sql)? {
            tx.execute_statement(&statement)?;
            if !tx.transaction_is_active() {
                return Err(Error::History(
                    "shadow transaction ended unexpectedly while verifying the proposal".into(),
                ));
            }
        }
    }
    let verified = catalog(|q| tx.query(q), "public", false)?;
    if verified != desired {
        return Err(Error::History(
            "generated migration does not reproduce the desired scalar schema".into(),
        ));
    }
    // Do not rely on Drop for the successful case: report rollback errors.
    tx.rollback()?;
    Ok(Plan {
        sql,
        basis: files
            .iter()
            .map(|f| (f.name.clone(), f.checksum.clone()))
            .collect(),
    })
}
/// Persist a nonempty plan only if the files used to create it are unchanged.
pub fn write_plan(dir: &Path, name: &str, plan: &Plan) -> Result<PathBuf, Error> {
    if plan.is_empty() {
        return Err(Error::Invalid("no schema changes to write".into()));
    }
    let _file_lock = FileLock::acquire(dir)?;
    let current = load(dir)?;
    let basis: Vec<_> = current
        .iter()
        .map(|f| (f.name.clone(), f.checksum.clone()))
        .collect();
    if basis != plan.basis {
        return Err(Error::History(
            "migration files changed after planning".into(),
        ));
    }
    write_file(dir, name, &plan.sql, &current)
}
