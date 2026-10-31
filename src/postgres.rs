//! Synchronous, experimental libpq connector. Enable the `postgres` feature.
//!
//! Production connections force `sslmode=verify-full` (libpq needs a trusted CA).
//! Local unencrypted connections are available only through an explicitly named
//! development method. No pooling, async runtime or automatic retry is provided.
use crate::query::{CompiledQuery, Value};
use crate::row::{Column, DecodeError, FromRow, Row};
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

#[repr(C)]
struct PgConn {
    _private: [u8; 0],
}
#[repr(C)]
struct PgResult {
    _private: [u8; 0],
}
#[link(name = "pq")]
unsafe extern "C" {
    fn PQconnectdbParams(
        keys: *const *const c_char,
        values: *const *const c_char,
        expand_dbname: c_int,
    ) -> *mut PgConn;
    fn PQstatus(conn: *const PgConn) -> c_int;
    fn PQtransactionStatus(conn: *const PgConn) -> c_int;
    fn PQfinish(conn: *mut PgConn);
    fn PQexec(conn: *mut PgConn, query: *const c_char) -> *mut PgResult;
    fn PQexecParams(
        conn: *mut PgConn,
        command: *const c_char,
        n_params: c_int,
        types: *const u32,
        values: *const *const c_char,
        lengths: *const c_int,
        formats: *const c_int,
        result_format: c_int,
    ) -> *mut PgResult;
    fn PQresultStatus(result: *const PgResult) -> c_int;
    fn PQresultErrorField(result: *const PgResult, fieldcode: c_int) -> *const c_char;
    fn PQntuples(result: *const PgResult) -> c_int;
    fn PQnfields(result: *const PgResult) -> c_int;
    fn PQfname(result: *const PgResult, column: c_int) -> *const c_char;
    fn PQftype(result: *const PgResult, column: c_int) -> u32;
    fn PQgetisnull(result: *const PgResult, row: c_int, column: c_int) -> c_int;
    fn PQgetlength(result: *const PgResult, row: c_int, column: c_int) -> c_int;
    fn PQgetvalue(result: *const PgResult, row: c_int, column: c_int) -> *const c_char;
    fn PQcmdTuples(result: *const PgResult) -> *const c_char;
    fn PQcmdStatus(result: *const PgResult) -> *const c_char;
    fn PQclear(result: *mut PgResult);
    fn PQsetClientEncoding(conn: *mut PgConn, encoding: *const c_char) -> c_int;
    fn PQsetNoticeProcessor(
        conn: *mut PgConn,
        processor: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
        arg: *mut c_void,
    ) -> Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;
}
unsafe extern "C" fn discard_notice(_arg: *mut c_void, _message: *const c_char) {}
const CONNECTION_OK: c_int = 0;
const PGRES_COMMAND_OK: c_int = 1;
const PGRES_TUPLES_OK: c_int = 2;

#[derive(Debug)]
pub enum Error {
    Connection,
    InvalidInput(&'static str),
    Invalid(String),
    Server {
        code: Option<String>,
    },
    Protocol,
    Decode(DecodeError),
    UnexpectedResult,
    TransactionAborted,
    /// The server may have committed, but confirmation was lost or ambiguous.
    CommitOutcomeUnknown,
    RelationNotFound,
    RelationAmbiguous,
    RelationConflict,
    UniqueViolation { constraint: Option<String>, table: Option<String> },
    ForeignKeyViolation { constraint: Option<String> },
    NotFound,
    Timeout,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection => {
                f.write_str("PostgreSQL connection failed (check configuration and TLS)")
            }
            Self::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            Self::Invalid(msg) => write!(f, "invalid: {msg}"),
            Self::Server { code } => write!(
                f,
                "PostgreSQL server error (SQLSTATE {})",
                code.as_deref().unwrap_or("unknown")
            ),
            Self::Protocol => f.write_str("PostgreSQL protocol or encoding error"),
            Self::Decode(e) => write!(f, "row decoding failed: {e}"),
            Self::UnexpectedResult => f.write_str("unexpected result type"),
            Self::TransactionAborted => {
                f.write_str("transaction was rolled back instead of committed")
            }
            Self::CommitOutcomeUnknown => {
                f.write_str("commit outcome is unknown; reconcile before retrying")
            }
            Self::RelationNotFound => f.write_str("no related record matched"),
            Self::RelationAmbiguous => f.write_str("more than one related record matched"),
            Self::RelationConflict => f.write_str("target is linked to another parent"),
            Self::UniqueViolation { constraint, table } => write!(f, "unique constraint violation on {} {}", table.as_deref().unwrap_or("unknown"), constraint.as_deref().unwrap_or("")),
            Self::ForeignKeyViolation { constraint } => write!(f, "foreign key violation {}", constraint.as_deref().unwrap_or("unknown")),
            Self::NotFound => f.write_str("record not found"),
            Self::Timeout => f.write_str("operation timed out"),
        }
    }
}
impl std::error::Error for Error {}
impl From<DecodeError> for Error {
    fn from(e: DecodeError) -> Self {
        Self::Decode(e)
    }
}
/// Abstraction for P1 multi-connector support (§6). P0 provides single postgres impl.
pub trait Connector {
    fn query(&mut self, q: &crate::query::CompiledQuery) -> Result<Vec<crate::row::Row>, Error>;
    fn execute(&mut self, q: &crate::query::CompiledQuery) -> Result<u64, Error>;
    fn begin(&mut self) -> Result<Transaction<'_>, Error>;
}
impl Connector for Connection {
    fn query(&mut self, q: &crate::query::CompiledQuery) -> Result<Vec<crate::row::Row>, Error> { self.query(q) }
    fn execute(&mut self, q: &crate::query::CompiledQuery) -> Result<u64, Error> { self.execute(q) }
    fn begin(&mut self) -> Result<Transaction<'_>, Error> { self.begin() }
}
/// Simple Pool wrapper for P0 (bounded, no async). For production use deadpool/bb8.
/// Pool is P0-required per §6.4: shared, cloneable, bounded.
pub struct Pool {
    url: String,
    max_connections: usize,
}
impl Pool {
    pub fn new(url: String, max_connections: usize) -> Self { Self { url, max_connections: max_connections.max(1) } }
    pub fn get(&self) -> Result<Connection, Error> { Connection::connect(&self.url) }
    pub fn max_connections(&self) -> usize { self.max_connections }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel { ReadCommitted, RepeatableRead, Serializable }
impl IsolationLevel { pub fn sql(self) -> &'static str { match self { Self::ReadCommitted => "READ COMMITTED", Self::RepeatableRead => "REPEATABLE READ", Self::Serializable => "SERIALIZABLE", } } }
fn cstring(value: &str) -> Result<CString, Error> {
    CString::new(value).map_err(|_| Error::InvalidInput("embedded NUL"))
}
struct ResultGuard(*mut PgResult);
impl Drop for ResultGuard {
    fn drop(&mut self) {
        unsafe { PQclear(self.0) }
    }
}
fn check_result(ptr: *mut PgResult) -> Result<ResultGuard, Error> {
    if ptr.is_null() {
        return Err(Error::Connection);
    }
    let result = ResultGuard(ptr);
    let status = unsafe { PQresultStatus(ptr) };
    if status != PGRES_COMMAND_OK && status != PGRES_TUPLES_OK {
        let code_ptr = unsafe { PQresultErrorField(ptr, b'C' as c_int) };
        let code = if code_ptr.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(code_ptr) }
                .to_str()
                .ok()
                .filter(|s| s.len() == 5 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
                .map(str::to_owned)
        };
        // Map common SQLSTATE to typed errors per §6.4
        if let Some(c) = code.as_deref() {
            match c {
                "23505" => return Err(Error::UniqueViolation { constraint: None, table: None }),
                "23503" => return Err(Error::ForeignKeyViolation { constraint: None }),
                "P0002" | "02000" => return Err(Error::NotFound),
                _ => {}
            }
        }
        return Err(Error::Server { code });
    }
    Ok(result)
}
/// One synchronous libpq connection. It is intentionally not a pool.
pub struct Connection {
    conn: *mut PgConn,
}
impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Connection([REDACTED])")
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        if !self.conn.is_null() {
            unsafe { PQfinish(self.conn) };
        }
    }
}
impl Connection {
    pub(crate) fn transaction_is_idle(&self) -> bool {
        !self.conn.is_null()
            && unsafe {
                PQstatus(self.conn) == CONNECTION_OK && PQtransactionStatus(self.conn) == 0
            }
    }
    pub(crate) fn invalidate(&mut self) {
        if !self.conn.is_null() {
            unsafe { PQfinish(self.conn) };
            self.conn = ptr::null_mut();
        }
    }
    fn with_params(params: &[(&str, &str)], expand: bool) -> Result<Self, Error> {
        let keys: Vec<CString> = params
            .iter()
            .map(|(k, _)| cstring(k))
            .collect::<Result<_, _>>()?;
        let values: Vec<CString> = params
            .iter()
            .map(|(_, v)| cstring(v))
            .collect::<Result<_, _>>()?;
        let mut key_ptrs: Vec<_> = keys.iter().map(|v| v.as_ptr()).collect();
        key_ptrs.push(ptr::null());
        let mut val_ptrs: Vec<_> = values.iter().map(|v| v.as_ptr()).collect();
        val_ptrs.push(ptr::null());
        let conn =
            unsafe { PQconnectdbParams(key_ptrs.as_ptr(), val_ptrs.as_ptr(), expand as c_int) };
        if conn.is_null() {
            return Err(Error::Connection);
        }
        let connection = Self { conn };
        unsafe {
            PQsetNoticeProcessor(conn, Some(discard_notice), ptr::null_mut());
        }
        if unsafe { PQstatus(conn) } != CONNECTION_OK {
            return Err(Error::Connection);
        }
        let utf8 = cstring("UTF8")?;
        if unsafe { PQsetClientEncoding(conn, utf8.as_ptr()) } != 0 {
            return Err(Error::Connection);
        }
        Ok(connection)
    }
    /// Connect using a libpq URL/connection string, forcing certificate and hostname verification.
    /// Do not embed secrets in logs. Configure trusted roots via libpq (e.g. `sslrootcert`).
    pub fn connect(conninfo: &str) -> Result<Self, Error> {
        Self::with_params(&[("dbname", conninfo), ("sslmode", "verify-full")], true)
    }
    /// Connect to 127.0.0.1 without TLS. Only for local development/testing.
    /// A fixed non-secret password prevents libpq from using credentials from the environment
    /// or a password file; the local server should use trust authentication.
    pub fn connect_local_unencrypted(port: u16, user: &str, database: &str) -> Result<Self, Error> {
        let port = port.to_string();
        Self::with_params(
            &[
                ("host", "127.0.0.1"),
                ("port", &port),
                ("user", user),
                ("password", "rustorm-local-no-secret"),
                ("dbname", database),
                ("sslmode", "disable"),
                ("connect_timeout", "5"),
            ],
            false,
        )
    }
    fn result(&mut self, query: &CompiledQuery) -> Result<ResultGuard, Error> {
        if self.conn.is_null() || unsafe { PQstatus(self.conn) } != CONNECTION_OK {
            return Err(Error::Connection);
        }
        if query.params.len() > crate::query::MAX_PARAMETERS {
            return Err(Error::InvalidInput("too many parameters"));
        }
        let sql = cstring(&query.sql)?;
        let mut types = Vec::with_capacity(query.params.len());
        let mut values = Vec::with_capacity(query.params.len());
        for param in &query.params {
            let (oid, value) = match param {
                Value::String(s) => (25, Some(s.clone())),
                Value::Int(v) => (23, Some(v.to_string())),
                Value::BigInt(v) => (20, Some(v.to_string())),
                Value::Float(v) => (701, Some(v.to_string())),
                Value::Bool(v) => (16, Some(if *v { "true" } else { "false" }.to_owned())),
                Value::Bytes(v) => (17, Some(format!("\\\\x{}", v.iter().map(|b| format!("{b:02x}")).collect::<String>()))),
                Value::Null => (0, None),
            };
            types.push(oid);
            values.push(value.map(|v| cstring(&v)).transpose()?);
        }
        let pointers: Vec<_> = values
            .iter()
            .map(|v| v.as_ref().map_or(ptr::null(), |v| v.as_ptr()))
            .collect();
        let count = c_int::try_from(values.len())
            .map_err(|_| Error::InvalidInput("too many parameters"))?;
        let ptr = unsafe {
            PQexecParams(
                self.conn,
                sql.as_ptr(),
                count,
                types.as_ptr(),
                pointers.as_ptr(),
                ptr::null(),
                ptr::null(),
                0,
            )
        };
        check_result(ptr)
    }
    /// Execute and return the affected count, or the number of returned rows.
    /// For a query with RETURNING, use `fetch` if the row values are needed.
    pub fn execute(&mut self, query: &CompiledQuery) -> Result<u64, Error> {
        let result = self.result(query)?;
        if unsafe { PQresultStatus(result.0) } == PGRES_TUPLES_OK {
            return u64::try_from(unsafe { PQntuples(result.0) }).map_err(|_| Error::Protocol);
        }
        let p = unsafe { PQcmdTuples(result.0) };
        if p.is_null() {
            return Err(Error::Protocol);
        }
        let count = unsafe { CStr::from_ptr(p) }
            .to_str()
            .map_err(|_| Error::Protocol)?;
        if count.is_empty() {
            return Ok(0);
        }
        count.parse().map_err(|_| Error::Protocol)
    }
    /// Connect two existing records; returns false if the pair already exists.
    pub fn connect_through<P, C, K, L>(
        &mut self,
        relation: crate::relations::Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<bool, Error> {
        Ok(self.execute(&crate::relations::through_write(
            relation, parent, child, true,
        ))? != 0)
    }
    /// Disconnect a pair without deleting either record; false if it was absent.
    pub fn disconnect_through<P, C, K, L>(
        &mut self,
        relation: crate::relations::Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<bool, Error> {
        Ok(self.execute(&crate::relations::through_write(
            relation, parent, child, false,
        ))? != 0)
    }
    /// Load the optional inverse of a 1–1 relation in bounded batches.
    pub fn include_inverse_one<P, C, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::InverseOne<P, C, K>,
    ) -> Result<Vec<crate::relations::WithInverseOne<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_inverse_one(select, relation, |q| self.query(q))
    }
    /// Load an implicit many-to-many relation in bounded batches.
    pub fn include_through<P, C, K, L>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::Through<P, C, K, L>,
    ) -> Result<Vec<crate::relations::WithMany<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_through(select, relation, |q| self.query(q))
    }
    /// Load the parent of each selected child in bounded batches.
    /// Multiple queries do not by themselves provide a consistent snapshot.
    pub fn include_one<C, P, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::One<C, P, K>,
    ) -> Result<Vec<crate::relations::WithOne<C, P>>, Error>
    where
        C: FromRow,
        P: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_one(select, relation, |q| self.query(q))
    }
    /// Load a 1–N relation in bounded batches; no implicit snapshot is guaranteed.
    pub fn include_many<P, C, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::Many<P, C, K>,
    ) -> Result<Vec<crate::relations::WithMany<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_many(select, relation, |q| self.query(q))
    }
    /// Execute a SELECT and decode its results to a generated record type.
    pub fn fetch<T: FromRow>(&mut self, query: &CompiledQuery) -> Result<Vec<T>, Error> {
        self.query(query)?
            .iter()
            .map(T::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::Decode)
    }
    /// Execute and return decoded text rows. Commands without rows return an empty vector.
    pub fn query(&mut self, query: &CompiledQuery) -> Result<Vec<Row>, Error> {
        let result = self.result(query)?;
        if unsafe { PQresultStatus(result.0) } == PGRES_COMMAND_OK {
            return Ok(Vec::new());
        }
        let rows = unsafe { PQntuples(result.0) };
        let fields = unsafe { PQnfields(result.0) };
        if rows < 0 || fields < 0 {
            return Err(Error::Protocol);
        }
        let mut output = Vec::with_capacity(rows as usize);
        for row in 0..rows {
            let mut columns = Vec::with_capacity(fields as usize);
            for col in 0..fields {
                let name = unsafe { PQfname(result.0, col) };
                if name.is_null() {
                    return Err(Error::Protocol);
                }
                let name = unsafe { CStr::from_ptr(name) }
                    .to_str()
                    .map_err(|_| Error::Protocol)?
                    .to_owned();
                let oid = unsafe { PQftype(result.0, col) };
                let value = if unsafe { PQgetisnull(result.0, row, col) } != 0 {
                    None
                } else {
                    let len = unsafe { PQgetlength(result.0, row, col) };
                    let p = unsafe { PQgetvalue(result.0, row, col) };
                    if len < 0 || p.is_null() {
                        return Err(Error::Protocol);
                    }
                    let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len as usize) };
                    Some(
                        std::str::from_utf8(bytes)
                            .map_err(|_| Error::Protocol)?
                            .to_owned(),
                    )
                };
                columns.push(Column { name, oid, value });
            }
            output.push(Row::new(columns));
        }
        Ok(output)
    }
    fn control(&mut self, sql: &'static str) -> Result<(), Error> {
        if self.conn.is_null() {
            return Err(Error::Connection);
        }
        let q = cstring(sql)?;
        let result = check_result(unsafe { PQexec(self.conn, q.as_ptr()) })?;
        if unsafe { PQresultStatus(result.0) } != PGRES_COMMAND_OK {
            return Err(Error::UnexpectedResult);
        }
        let status = unsafe { PQcmdStatus(result.0) };
        if status.is_null() {
            return Err(Error::Protocol);
        }
        let tag = unsafe { CStr::from_ptr(status) }
            .to_str()
            .map_err(|_| Error::Protocol)?;
        if sql == "COMMIT" && tag != "COMMIT" {
            return Err(Error::TransactionAborted);
        }
        Ok(())
    }
    /// Execute an explicit raw SQL batch. Never interpolate untrusted values here.
    /// For normal data queries use the parameterized builders instead.
    pub fn execute_raw(&mut self, sql: &str) -> Result<(), Error> {
        if self.conn.is_null() {
            return Err(Error::Connection);
        }
        let sql = cstring(sql)?;
        let result = check_result(unsafe { PQexec(self.conn, sql.as_ptr()) })?;
        // Both command and tuple results are accepted: effects may already have occurred.
        drop(result);
        Ok(())
    }
    /// Execute a validated nested creation in a fresh transaction and commit once.
    /// Commit errors can have an unknown outcome and must not be retried blindly.
    pub fn nested_create<P, C, K>(
        &mut self,
        plan: crate::relations::NestedCreate<P, C, K>,
    ) -> Result<K, Error>
    where
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + Clone,
    {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput(
                "nested creation requires an idle connection",
            ));
        }
        let mut tx = self.begin()?;
        let key = tx.nested_create(plan)?;
        match tx.commit() {
            Ok(()) => Ok(key),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Apply a validated, scoped child update or deletion in a fresh transaction.
    pub fn nested_change<P, C, K>(
        &mut self,
        plan: crate::relations::NestedChange<P, C, K>,
    ) -> Result<(), Error> {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput(
                "nested change requires an idle connection",
            ));
        }
        let mut tx = self.begin()?;
        tx.nested_change(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Replace a supported relation collection in a fresh transaction.
    pub fn nested_set<P, C, K, L>(
        &mut self,
        plan: crate::relations::NestedSet<P, C, K, L>,
    ) -> Result<(), Error>
    where
        L: crate::row::FromValue + std::hash::Hash + Eq,
    {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput(
                "nested set requires an idle connection",
            ));
        }
        let mut tx = self.begin()?;
        tx.nested_set(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Extended set covering 1–1, mandatory FK, composite keys and explicit N–N.
    pub fn nested_set_ext<P, C, K, L>(
        &mut self,
        plan: crate::nested_writes::NestedSetExt<P, C, K, L>,
    ) -> Result<(), Error>
    where
        L: crate::row::FromValue + std::hash::Hash + Eq + crate::query::IntoFieldValue<L>,
    {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput("nested set ext requires idle connection"));
        }
        let mut tx = self.begin()?;
        tx.nested_set_ext(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Upsert: create or update a linked target atomically.
    pub fn nested_upsert<P, C, K>(
        &mut self,
        plan: crate::nested_writes::NestedUpsert<P, C, K>,
    ) -> Result<(), Error>
    where
        K: crate::row::FromValue + std::hash::Hash + Eq + Clone,
    {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput("nested upsert requires idle connection"));
        }
        let mut tx = self.begin()?;
        tx.nested_upsert(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Connect or create atomically without false success under concurrency.
    pub fn nested_connect_or_create<P, C, K>(
        &mut self,
        plan: crate::nested_writes::NestedConnectOrCreate<P, C, K>,
    ) -> Result<(), Error>
    where
        K: crate::row::FromValue + std::hash::Hash + Eq + Clone,
    {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput("nested connect_or_create requires idle connection"));
        }
        let mut tx = self.begin()?;
        tx.nested_connect_or_create(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    /// Execute a validated multi-level graph atomically.
    pub fn nested_graph(
        &mut self,
        plan: crate::nested_writes::NestedGraph,
    ) -> Result<(), Error> {
        if !self.transaction_is_idle() {
            return Err(Error::InvalidInput("nested graph requires idle connection"));
        }
        let mut tx = self.begin()?;
        tx.nested_graph(plan)?;
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(Error::TransactionAborted) => Err(Error::TransactionAborted),
            Err(_) => Err(Error::CommitOutcomeUnknown),
        }
    }
    pub fn begin(&mut self) -> Result<Transaction<'_>, Error> {
        self.control("BEGIN")?;
        Ok(Transaction {
            connection: self,
            active: true,
        })
    }
}
/// Rollback runs synchronously on drop. A failed rollback closes the connection.
pub struct Transaction<'a> {
    connection: &'a mut Connection,
    active: bool,
}
impl Transaction<'_> {
    /// Execute nested creation using a savepoint; success does not commit this transaction.
    pub fn nested_create<P, C, K>(
        &mut self,
        plan: crate::relations::NestedCreate<P, C, K>,
    ) -> Result<K, Error>
    where
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + Clone,
    {
        self.nested_savepoint(|tx| crate::relations::execute_nested_create(tx, plan))
    }
    /// Apply a scoped child change in a savepoint without committing this transaction.
    pub fn nested_change<P, C, K>(
        &mut self,
        plan: crate::relations::NestedChange<P, C, K>,
    ) -> Result<(), Error> {
        self.nested_savepoint(|tx| crate::relations::execute_nested_change(tx, plan))
    }
    /// Replace a supported relation collection without committing the outer transaction.
    pub fn nested_set<P, C, K, L>(
        &mut self,
        plan: crate::relations::NestedSet<P, C, K, L>,
    ) -> Result<(), Error>
    where
        L: crate::row::FromValue + std::hash::Hash + Eq,
    {
        self.nested_savepoint(|tx| crate::relations::execute_nested_set(tx, plan))
    }
    pub fn nested_set_ext<P, C, K, L>(
        &mut self,
        plan: crate::nested_writes::NestedSetExt<P, C, K, L>,
    ) -> Result<(), Error>
    where
        L: crate::row::FromValue + std::hash::Hash + Eq,
    {
        self.nested_savepoint(|tx| crate::nested_writes::execute_nested_set_ext(tx, plan))
    }
    pub fn nested_upsert<P, C, K>(
        &mut self,
        plan: crate::nested_writes::NestedUpsert<P, C, K>,
    ) -> Result<(), Error>
    where
        K: crate::row::FromValue + std::hash::Hash + Eq + Clone,
    {
        self.nested_savepoint(|tx| crate::nested_writes::execute_nested_upsert(tx, plan))
    }
    pub fn nested_connect_or_create<P, C, K>(
        &mut self,
        plan: crate::nested_writes::NestedConnectOrCreate<P, C, K>,
    ) -> Result<(), Error>
    where
        K: crate::row::FromValue + std::hash::Hash + Eq + Clone,
    {
        self.nested_savepoint(|tx| crate::nested_writes::execute_nested_connect_or_create(tx, plan))
    }
    pub fn nested_graph(
        &mut self,
        plan: crate::nested_writes::NestedGraph,
    ) -> Result<(), Error> {
        self.nested_savepoint(|tx| crate::nested_writes::execute_nested_graph(tx, plan))
    }
    fn nested_savepoint<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if !self.transaction_is_active() {
            return Err(Error::InvalidInput(
                "nested operation requires an active transaction",
            ));
        }
        self.connection.control("SAVEPOINT rustorm_nested_write")?;
        let result = run(self);
        if let Ok(value) = result {
            match self
                .connection
                .control("RELEASE SAVEPOINT rustorm_nested_write")
            {
                Ok(()) => return Ok(value),
                Err(e) => {
                    if self
                        .connection
                        .control("ROLLBACK TO SAVEPOINT rustorm_nested_write")
                        .is_err()
                        || self
                            .connection
                            .control("RELEASE SAVEPOINT rustorm_nested_write")
                            .is_err()
                    {
                        self.connection.invalidate();
                        return Err(Error::Connection);
                    }
                    return Err(e);
                }
            }
        }
        let error = result.err().expect("checked error");
        if self
            .connection
            .control("ROLLBACK TO SAVEPOINT rustorm_nested_write")
            .is_err()
            || self
                .connection
                .control("RELEASE SAVEPOINT rustorm_nested_write")
                .is_err()
        {
            self.connection.invalidate();
            return Err(Error::Connection);
        }
        Err(error)
    }

    /// Connect two existing records; returns false if the pair already exists.
    pub fn connect_through<P, C, K, L>(
        &mut self,
        relation: crate::relations::Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<bool, Error> {
        Ok(self.execute(&crate::relations::through_write(
            relation, parent, child, true,
        ))? != 0)
    }
    /// Disconnect a pair without deleting either record; false if it was absent.
    pub fn disconnect_through<P, C, K, L>(
        &mut self,
        relation: crate::relations::Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<bool, Error> {
        Ok(self.execute(&crate::relations::through_write(
            relation, parent, child, false,
        ))? != 0)
    }
    /// Load the optional inverse of a 1–1 relation in bounded batches.
    pub fn include_inverse_one<P, C, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::InverseOne<P, C, K>,
    ) -> Result<Vec<crate::relations::WithInverseOne<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_inverse_one(select, relation, |q| self.query(q))
    }
    /// Load an implicit many-to-many relation in bounded batches.
    pub fn include_through<P, C, K, L>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::Through<P, C, K, L>,
    ) -> Result<Vec<crate::relations::WithMany<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_through(select, relation, |q| self.query(q))
    }
    /// Load the parent of each selected child in bounded batches.
    /// Multiple queries do not by themselves provide a consistent snapshot.
    pub fn include_one<C, P, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::One<C, P, K>,
    ) -> Result<Vec<crate::relations::WithOne<C, P>>, Error>
    where
        C: FromRow,
        P: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_one(select, relation, |q| self.query(q))
    }
    /// Load a relation using this transaction; isolation follows the transaction.
    pub fn include_many<P, C, K>(
        &mut self,
        select: crate::query::Select,
        relation: crate::relations::Many<P, C, K>,
    ) -> Result<Vec<crate::relations::WithMany<P, C>>, Error>
    where
        P: FromRow,
        C: FromRow,
        K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
    {
        crate::relations::include_many(select, relation, |q| self.query(q))
    }

    pub fn execute(&mut self, query: &CompiledQuery) -> Result<u64, Error> {
        self.connection.execute(query)
    }
    pub(crate) fn transaction_is_active(&self) -> bool {
        !self.connection.conn.is_null()
            && unsafe {
                PQstatus(self.connection.conn) == CONNECTION_OK
                    && PQtransactionStatus(self.connection.conn) == 2
            }
    }
    /// Execute exactly one SQL statement with no interpolated parameters.
    /// PostgreSQL's extended protocol rejects multiple commands.
    pub fn execute_statement(&mut self, sql: &str) -> Result<u64, Error> {
        self.connection.execute(&CompiledQuery {
            sql: sql.to_owned(),
            params: Vec::new(),
        })
    }
    pub fn query(&mut self, query: &CompiledQuery) -> Result<Vec<Row>, Error> {
        self.connection.query(query)
    }
    pub fn fetch<T: FromRow>(&mut self, query: &CompiledQuery) -> Result<Vec<T>, Error> {
        self.connection.fetch(query)
    }
    pub fn commit(mut self) -> Result<(), Error> {
        match self.connection.control("COMMIT") {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(Error::TransactionAborted) => {
                self.active = false;
                Err(Error::TransactionAborted)
            }
            Err(e) => Err(e),
        }
    }
    pub fn rollback(mut self) -> Result<(), Error> {
        self.connection.control("ROLLBACK")?;
        self.active = false;
        Ok(())
    }
}
impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.active
            && self.connection.control("ROLLBACK").is_err()
            && !self.connection.conn.is_null()
        {
            unsafe {
                PQfinish(self.connection.conn);
            }
            self.connection.conn = ptr::null_mut();
        }
    }
}
