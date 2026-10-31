//! Typed decoding of PostgreSQL text-format result rows.
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub oid: u32,
    pub value: Option<String>,
}
impl fmt::Debug for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Column")
            .field("name", &self.name)
            .field("oid", &self.oid)
            .field("value", &"[REDACTED]")
            .finish()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    columns: Vec<Column>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    MissingColumn(String),
    AmbiguousColumn(String),
    UnexpectedNull,
    UnexpectedType { expected: &'static str, oid: u32 },
    InvalidValue(&'static str),
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingColumn(s) => write!(f, "missing column {s:?}"),
            Self::AmbiguousColumn(s) => write!(f, "ambiguous column {s:?}"),
            Self::UnexpectedNull => f.write_str("unexpected SQL NULL"),
            Self::UnexpectedType { expected, oid } => {
                write!(f, "expected {expected}, got PostgreSQL type OID {oid}")
            }
            Self::InvalidValue(ty) => write!(f, "invalid value for {ty}"),
        }
    }
}
impl std::error::Error for DecodeError {}
impl Row {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    pub fn get<T: FromValue>(&self, name: &str) -> Result<T, DecodeError> {
        let mut matches = self.columns.iter().filter(|c| c.name == name);
        let col = matches
            .next()
            .ok_or_else(|| DecodeError::MissingColumn(name.into()))?;
        if matches.next().is_some() {
            return Err(DecodeError::AmbiguousColumn(name.into()));
        }
        T::from_value(col.oid, col.value.as_deref())
    }
}
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, DecodeError>;
}
pub trait FromValue: Sized {
    fn from_value(oid: u32, value: Option<&str>) -> Result<Self, DecodeError>;
}
fn required<'a>(
    oid: u32,
    value: Option<&'a str>,
    accepted: &[u32],
    name: &'static str,
) -> Result<&'a str, DecodeError> {
    if !accepted.contains(&oid) {
        return Err(DecodeError::UnexpectedType {
            expected: name,
            oid,
        });
    }
    value.ok_or(DecodeError::UnexpectedNull)
}
macro_rules! number {
    ($ty:ty, $oid:expr, $label:literal) => {
        impl FromValue for $ty {
            fn from_value(oid: u32, value: Option<&str>) -> Result<Self, DecodeError> {
                required(oid, value, &[$oid], $label)?
                    .parse()
                    .map_err(|_| DecodeError::InvalidValue($label))
            }
        }
    };
}
number!(i32, 23, "Int");
number!(i64, 20, "BigInt");
number!(f64, 701, "Float");
impl FromValue for bool {
    fn from_value(oid: u32, value: Option<&str>) -> Result<Self, DecodeError> {
        match required(oid, value, &[16], "Boolean")? {
            "t" => Ok(true),
            "f" => Ok(false),
            _ => Err(DecodeError::InvalidValue("Boolean")),
        }
    }
}
impl FromValue for String {
    fn from_value(oid: u32, value: Option<&str>) -> Result<Self, DecodeError> {
        Ok(required(oid, value, &[25, 1042, 1043], "String")?.to_owned())
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(oid: u32, value: Option<&str>) -> Result<Self, DecodeError> {
        if value.is_none() {
            // Type-check NULL as well, without accepting a value for a different type.
            match T::from_value(oid, None) {
                Err(DecodeError::UnexpectedNull) => Ok(None),
                Err(e) => Err(e),
                Ok(_) => Err(DecodeError::InvalidValue("nullable value")),
            }
        } else {
            T::from_value(oid, value).map(Some)
        }
    }
}
