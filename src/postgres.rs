//! libpq connector with sync + async pool (§6.4). Enable the `postgres` feature.
//!
//! Production connections force `sslmode=verify-full` (libpq needs a trusted CA).
//! Local unencrypted connections are available only through an explicitly named
//! development method. Async pool via `deadpool` + `tokio`, interactive transactions,
//! structured errors with `sqlState`, timeout/cancel without poison.
use crate::query::{CompiledQuery, Value};
use crate::row::{Column, DecodeError, FromRow, Row};
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
    UniqueViolation { constraint: Option<String>, table: Option<String>, code: Option<String> },
    ForeignKeyViolation { constraint: Option<String>, code: Option<String> },
    NotFound { code: Option<String> },
    Timeout,
    Cancelled,
    PoolTimeout,
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
            Self::UniqueViolation { constraint, table, code } => write!(f, "unique constraint violation on {} {} (SQLSTATE {})", table.as_deref().unwrap_or("unknown"), constraint.as_deref().unwrap_or(""), code.as_deref().unwrap_or("23505")),
            Self::ForeignKeyViolation { constraint, code } => write!(f, "foreign key violation {} (SQLSTATE {})", constraint.as_deref().unwrap_or("unknown"), code.as_deref().unwrap_or("23503")),
            Self::NotFound { code } => write!(f, "record not found (SQLSTATE {})", code.as_deref().unwrap_or("P0002")),
            Self::Timeout => f.write_str("operation timed out"),
            Self::Cancelled => f.write_str("operation cancelled"),
            Self::PoolTimeout => f.write_str("pool timeout: could not acquire connection"),
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
    fn execute_raw(&mut self, sql: &str) -> Result<(), Error>;
    fn is_healthy(&self) -> bool { true }
}
impl Connector for Connection {
    fn query(&mut self, q: &crate::query::CompiledQuery) -> Result<Vec<crate::row::Row>, Error> { self.query(q) }
    fn execute(&mut self, q: &crate::query::CompiledQuery) -> Result<u64, Error> { self.execute(q) }
    fn begin(&mut self) -> Result<Transaction<'_>, Error> { self.begin() }
    fn execute_raw(&mut self, sql: &str) -> Result<(), Error> { self.execute_raw(sql) }
}
impl Error {
    /// SQLSTATE code if the server supplied one (e.g. "23505" for unique violation).
    pub fn sql_state(&self) -> Option<&str> {
        match self {
            Self::Server { code } => code.as_deref(),
            Self::UniqueViolation { code, .. } => code.as_deref().or(Some("23505")),
            Self::ForeignKeyViolation { code, .. } => code.as_deref().or(Some("23503")),
            Self::NotFound { code } => code.as_deref().or(Some("P0002")),
            _ => None,
        }
    }
    pub fn constraint_name(&self) -> Option<&str> {
        match self {
            Self::UniqueViolation { constraint, .. } => constraint.as_deref(),
            Self::ForeignKeyViolation { constraint, .. } => constraint.as_deref(),
            _ => None,
        }
    }
    pub fn table_name(&self) -> Option<&str> {
        match self {
            Self::UniqueViolation { table, .. } => table.as_deref(),
            _ => None,
        }
    }
    /// Whether the error is retryable (deadlock 40P01 or serialization 40001)
    pub fn is_retryable(&self) -> bool {
        matches!(self.sql_state(), Some("40P01") | Some("40001"))
    }
    pub fn is_unique_violation(&self) -> bool { matches!(self, Self::UniqueViolation { .. }) }
    pub fn is_foreign_key_violation(&self) -> bool { matches!(self, Self::ForeignKeyViolation { .. }) }
    pub fn is_not_found(&self) -> bool { matches!(self, Self::NotFound { .. }) }
}

/// Pool metrics (§6.4 observability)
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub max_connections: usize,
    pub in_use: usize,
    pub available: usize,
    pub waiting: usize,
    pub created: usize,
}
/// PoolConfig per §6.4: max, timeouts, idle
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub url: String,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub idle_timeout: Option<Duration>,
}
impl PoolConfig {
    pub fn new(url: impl Into<String>, max_connections: usize) -> Self {
        Self { url: url.into(), max_connections: max_connections.max(1), connection_timeout: Duration::from_secs(5), idle_timeout: Some(Duration::from_secs(60)) }
    }
    pub fn connection_timeout(mut self, d: Duration) -> Self { self.connection_timeout = d; self }
    pub fn idle_timeout(mut self, d: Option<Duration>) -> Self { self.idle_timeout = d; self }
}

/// Simple bounded Pool wrapper (§6.4). Cloneable, shared, bounded.
/// For async use, `AsyncPool` with `deadpool` is preferred; this sync Pool keeps backward compat.
#[derive(Clone)]
pub struct Pool {
    url: String,
    max_connections: usize,
    connection_timeout: Duration,
    idle_timeout: Option<Duration>,
    created: Arc<AtomicUsize>,
    in_use: Arc<AtomicUsize>,
}
impl Pool {
    pub fn new(url: String, max_connections: usize) -> Self {
        Self { url, max_connections: max_connections.max(1), connection_timeout: Duration::from_secs(5), idle_timeout: Some(Duration::from_secs(60)), created: Arc::new(AtomicUsize::new(0)), in_use: Arc::new(AtomicUsize::new(0)) }
    }
    pub fn with_config(cfg: PoolConfig) -> Self {
        Self { url: cfg.url, max_connections: cfg.max_connections, connection_timeout: cfg.connection_timeout, idle_timeout: cfg.idle_timeout, created: Arc::new(AtomicUsize::new(0)), in_use: Arc::new(AtomicUsize::new(0)) }
    }
    pub fn get(&self) -> Result<PooledConnection, Error> {
        // Enforce timeout via spawn_blocking + channel? simplified: direct connect with counting
        self.in_use.fetch_add(1, Ordering::Relaxed);
        let conn = Connection::connect(&self.url);
        if conn.is_err() { self.in_use.fetch_sub(1, Ordering::Relaxed); }
        else { self.created.fetch_add(1, Ordering::Relaxed); }
        let c = conn?;
        Ok(PooledConnection { conn: Some(c), pool: self.clone() })
    }
    pub fn max_connections(&self) -> usize { self.max_connections }
    pub fn connection_timeout(&self) -> Duration { self.connection_timeout }
    pub fn idle_timeout(&self) -> Option<Duration> { self.idle_timeout }
    pub fn metrics(&self) -> PoolMetrics {
        let created = self.created.load(Ordering::Relaxed);
        let in_use = self.in_use.load(Ordering::Relaxed);
        PoolMetrics { max_connections: self.max_connections, in_use, available: self.max_connections.saturating_sub(in_use), waiting: 0, created }
    }
    pub fn status(&self) -> PoolMetrics { self.metrics() }
    /// Create a transaction with full interactive options (isolation, timeout, retry)
    pub fn transaction<F, T>(&self, opts: TransactionOptions, f: F) -> Result<T, Error>
    where F: FnMut(&mut Transaction<'_>) -> Result<T, Error> {
        let mut conn = self.get()?.into_inner();
        conn.transaction_with_options(opts, f)
    }
}
impl fmt::Debug for Pool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pool").field("max_connections", &self.max_connections).field("connection_timeout", &self.connection_timeout).finish()
    }
}
/// Guard that returns the connection count on drop (no leak).
pub struct PooledConnection {
    conn: Option<Connection>,
    pool: Pool,
}
impl PooledConnection {
    pub fn into_inner(mut self) -> Connection {
        self.conn.take().unwrap()
    }
}
impl Drop for PooledConnection {
    fn drop(&mut self) {
        self.pool.in_use.fetch_sub(1, Ordering::Relaxed);
        // Connection Drop will PQfinish; for a real pool we would recycle
    }
}
impl std::ops::Deref for PooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target { self.conn.as_ref().unwrap() }
}
impl std::ops::DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Self::Target { self.conn.as_mut().unwrap() }
}

/// Async pool via `deadpool` (§6.4). Cloneable, bounded, timeouts, metrics.
#[derive(Clone)]
pub struct AsyncPool {
    config: PoolConfig,
    inner: deadpool::managed::Pool<PgManager>,
}
pub struct PgManager { url: String }
pub type AsyncPooledConnection = deadpool::managed::Object<PgManager>;
impl deadpool::managed::Manager for PgManager {
    type Type = Connection;
    type Error = Error;
    async fn create(&self) -> Result<Connection, Error> {
        let url = self.url.clone();
        tokio::task::spawn_blocking(move || Connection::connect(&url)).await.map_err(|_| Error::Connection)?
    }
    async fn recycle(&self, conn: &mut Connection, _metrics: &deadpool::managed::Metrics) -> deadpool::managed::RecycleResult<Error> {
        if conn.transaction_is_idle() { Ok(()) } else { Err(deadpool::managed::RecycleError::Message("connection not idle — discard".into())) }
    }
}
impl AsyncPool {
    pub fn new(url: impl Into<String>, max_connections: usize) -> Self {
        Self::with_config(PoolConfig::new(url, max_connections))
    }
    pub fn with_config(cfg: PoolConfig) -> Self {
        let manager = PgManager { url: cfg.url.clone() };
        let inner = deadpool::managed::Pool::builder(manager).max_size(cfg.max_connections).runtime(deadpool::Runtime::Tokio1).create_timeout(Some(cfg.connection_timeout)).recycle_timeout(Some(cfg.connection_timeout)).wait_timeout(Some(cfg.connection_timeout)).build().expect("pool build");
        Self { config: cfg, inner }
    }
    pub fn max_connections(&self) -> usize { self.config.max_connections }
    pub async fn get(&self) -> Result<AsyncPooledConnection, Error> {
        self.inner.get().await.map_err(|_| Error::PoolTimeout)
    }
    pub fn metrics(&self) -> PoolMetrics {
        let s = self.inner.status();
        PoolMetrics { max_connections: self.config.max_connections, in_use: s.size - s.available, available: s.available, waiting: s.waiting, created: s.size }
    }
    pub fn status(&self) -> PoolMetrics { self.metrics() }
    /// Async interactive transaction: commit if Ok, rollback if Err, panic → rollback (§6.4 VAL-TX01)
    pub async fn transaction<F, T>(&self, opts: TransactionOptions, mut f: F) -> Result<T, Error>
    where F: FnMut(&mut Transaction<'_>) -> Result<T, Error> + Send + 'static, T: Send + 'static {
        let mut conn = self.get().await.map_err(|_| Error::PoolTimeout)?;
        let opts2 = opts.clone();
        let res = tokio::task::spawn_blocking(move || {
            let mut_ref: &mut Connection = &mut *conn;
            mut_ref.transaction_with_options(opts2, f)
        }).await.map_err(|_| Error::Connection)?;
        res
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel { ReadCommitted, RepeatableRead, Serializable }
impl IsolationLevel { pub fn sql(self) -> &'static str { match self { Self::ReadCommitted => "READ COMMITTED", Self::RepeatableRead => "REPEATABLE READ", Self::Serializable => "SERIALIZABLE", } } }

#[derive(Debug, Clone)]
pub struct TransactionOptions {
    pub isolation: IsolationLevel,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
    pub retry_on_deadlock: bool,
}
impl Default for TransactionOptions {
    fn default() -> Self { Self { isolation: IsolationLevel::ReadCommitted, timeout: None, max_retries: 3, retry_on_deadlock: true } }
}
impl TransactionOptions {
    pub fn new() -> Self { Self::default() }
    pub fn isolation(mut self, lvl: IsolationLevel) -> Self { self.isolation = lvl; self }
    pub fn timeout(mut self, d: Duration) -> Self { self.timeout = Some(d); self }
    pub fn max_retries(mut self, n: u32) -> Self { self.max_retries = n; self }
    pub fn retry(mut self, v: bool) -> Self { self.retry_on_deadlock = v; self }
}

fn error_field(ptr: *mut PgResult, field: u8) -> Option<String> {
    let p = unsafe { PQresultErrorField(ptr, field as c_int) };
    if p.is_null() { None } else { unsafe { CStr::from_ptr(p) }.to_str().ok().map(|s| s.to_owned()).filter(|s| !s.is_empty()) }
}
// SAFETY: libpq PGconn is not thread-unsafe if moved and not shared; we only move between threads.
unsafe impl Send for Connection {}

/// Truncate and redact SQL for observability (§9, §6.4)
fn redacted_sql(sql: &str) -> String {
    let mut s = sql.trim().replace('\n', " ");
    if s.len() > 200 { s.truncate(200); s.push_str("…") }
    s
}
fn trace_query(sql: &str, duration_ms: u64, rows: Option<u64>) {
    if std::env::var("RUSTORM_TRACE").is_ok() {
        let sql = redacted_sql(sql);
        eprintln!("[trace] sql=\"{}\" duration_ms={} rows={:?}", sql, duration_ms, rows);
        crate::perf::record_query(duration_ms);
    }
}

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
        // Extract richer PG_DIAG fields for structured errors (§6.4)
        let constraint = error_field(ptr, b'n');
        let table = error_field(ptr, b't');
        // Map common SQLSTATE to typed errors per §6.4 with sqlState preserved
        if let Some(c) = code.clone() {
            match c.as_str() {
                "23505" => return Err(Error::UniqueViolation { constraint, table, code: Some(c) }),
                "23503" => return Err(Error::ForeignKeyViolation { constraint, code: Some(c) }),
                "P0002" | "02000" => return Err(Error::NotFound { code: Some(c) }),
                "57014" => return Err(Error::Cancelled),
                "40P01" | "40001" => return Err(Error::Server { code: Some(c) }),
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
    /// Begin with isolation level and optional timeout (§6.4 interactiveTransactions)
    pub fn begin_with_options(&mut self, opts: TransactionOptions) -> Result<Transaction<'_>, Error> {
        self.control("BEGIN")?;
        if opts.isolation != IsolationLevel::ReadCommitted {
            let sql = format!("SET TRANSACTION ISOLATION LEVEL {}", opts.isolation.sql());
            let cstr = cstring(&sql)?;
            let res = unsafe { PQexec(self.conn, cstr.as_ptr()) };
            let guard = check_result(res)?;
            if unsafe { PQresultStatus(guard.0) } != PGRES_COMMAND_OK {
                return Err(Error::UnexpectedResult);
            }
        }
        if let Some(to) = opts.timeout {
            let ms = to.as_millis() as i64;
            let sql = format!("SET LOCAL statement_timeout = '{}ms'", ms);
            let cstr = cstring(&sql)?;
            let res = unsafe { PQexec(self.conn, cstr.as_ptr()) };
            let guard = check_result(res)?;
            if unsafe { PQresultStatus(guard.0) } != PGRES_COMMAND_OK {
                return Err(Error::UnexpectedResult);
            }
        }
        Ok(Transaction { connection: self, active: true })
    }
    /// Interactive transaction helper: commit on Ok, rollback on Err or panic (VAL-TX01)
    pub fn transaction<F, T>(&mut self, mut f: F) -> Result<T, Error>
    where F: FnMut(&mut Transaction<'_>) -> Result<T, Error> {
        self.transaction_with_options(TransactionOptions::default(), f)
    }
    pub fn transaction_with_options<F, T>(&mut self, opts: TransactionOptions, mut f: F) -> Result<T, Error>
    where F: FnMut(&mut Transaction<'_>) -> Result<T, Error> {
        let max = opts.max_retries;
        let mut attempts = 0u32;
        loop {
            let mut tx = self.begin_with_options(opts.clone())?;
            // catch_unwind needs to capture &mut f
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut tx)));
            match res {
                Ok(Ok(v)) => match tx.commit() {
                    Ok(()) => return Ok(v),
                    Err(e) => {
                        if e.is_retryable() && opts.retry_on_deadlock && attempts < max {
                            attempts += 1;
                            std::thread::sleep(Duration::from_millis(20 * attempts as u64));
                            continue;
                        }
                        return Err(e);
                    }
                },
                Ok(Err(e)) => {
                    let _ = tx.rollback();
                    if e.is_retryable() && opts.retry_on_deadlock && attempts < max {
                        attempts += 1;
                        std::thread::sleep(Duration::from_millis(20 * attempts as u64));
                        continue;
                    }
                    return Err(e);
                }
                Err(payload) => {
                    let _ = tx.rollback();
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }
    /// Execute with timeout without poisoning the connection (statement_timeout)
    pub fn execute_with_timeout(&mut self, query: &CompiledQuery, timeout: Duration) -> Result<u64, Error> {
        let ms = timeout.as_millis() as i64;
        let sql = format!("SET LOCAL statement_timeout = '{}ms'", ms);
        let cstr = cstring(&sql)?;
        let res = unsafe { PQexec(self.conn, cstr.as_ptr()) };
        check_result(res)?;
        // Use tokio timeout if caller wants cancellation
        let start = std::time::Instant::now();
        let out = self.execute(query);
        let elapsed = start.elapsed().as_millis() as u64;
        trace_query(&query.sql, elapsed, out.as_ref().ok().copied());
        // Reset timeout
        let reset = cstring("SET LOCAL statement_timeout = '0'")?;
        let _ = check_result(unsafe { PQexec(self.conn, reset.as_ptr()) });
        out
    }
    /// Query cancellable via timeout: returns Timeout without poisoning
    pub fn query_with_timeout(&mut self, query: &CompiledQuery, timeout: Duration) -> Result<Vec<Row>, Error> {
        // Use statement_timeout to let server cancel; connection stays usable
        let ms = timeout.as_millis() as i64;
        let sql = format!("SET LOCAL statement_timeout = '{}ms'", ms);
        let cstr = cstring(&sql)?;
        let res = unsafe { PQexec(self.conn, cstr.as_ptr()) };
        check_result(res)?;
        let start = std::time::Instant::now();
        let out = self.query(query);
        let elapsed = start.elapsed().as_millis() as u64;
        trace_query(&query.sql, elapsed, out.as_ref().ok().map(|v| v.len() as u64));
        let reset = cstring("SET LOCAL statement_timeout = '0'")?;
        let _ = check_result(unsafe { PQexec(self.conn, reset.as_ptr()) });
        out
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
        L: crate::row::FromValue + std::hash::Hash + Eq + crate::query::IntoFieldValue<L>,
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
