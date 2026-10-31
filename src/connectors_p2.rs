//! P2: Advanced connectors (CockroachDB, SQL Server, MongoDB)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind { Postgres, Cockroach, Mssql, Mongodb }

#[derive(Debug, Clone)]
pub struct CapabilityMatrix {
    pub transactions: bool,
    pub json: bool,
    pub enums: bool,
    pub views: bool,
    pub composite_types: bool,
}

impl ConnectorKind {
    pub fn capabilities(&self) -> CapabilityMatrix {
        match self {
            Self::Postgres => CapabilityMatrix { transactions: true, json: true, enums: true, views: true, composite_types: true },
            Self::Cockroach => CapabilityMatrix { transactions: true, json: true, enums: false, views: false, composite_types: false },
            Self::Mssql => CapabilityMatrix { transactions: true, json: false, enums: false, views: true, composite_types: false },
            Self::Mongodb => CapabilityMatrix { transactions: false, json: true, enums: false, views: false, composite_types: false },
        }
    }
    pub fn is_supported(&self, feature: &str) -> bool {
        let caps = self.capabilities();
        match feature {
            "transactions" => caps.transactions,
            "json" => caps.json,
            "enums" => caps.enums,
            "views" => caps.views,
            "composite" => caps.composite_types,
            _ => false,
        }
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "postgresql" | "postgres" | "pg" => Ok(Self::Postgres),
            "cockroach" | "cockroachdb" => Ok(Self::Cockroach),
            "mssql" | "sqlserver" | "sql-server" => Ok(Self::Mssql),
            "mongodb" | "mongo" => Ok(Self::Mongodb),
            _ => Err(format!("unsupported connector '{s}'")),
        }
    }
}

#[derive(Debug)]
pub struct ConnectorError(pub String);
impl std::fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "connector error: {}", self.0) }
}
impl std::error::Error for ConnectorError {}

pub fn check_schema_compatibility(kind: ConnectorKind, schema: &crate::schema::Schema) -> Result<(), ConnectorError> {
    if kind == ConnectorKind::Mongodb && schema.models.iter().any(|m| !m.relations.is_empty()) {
        return Err(ConnectorError("MongoDB connector does not support SQL relations".into()));
    }
    if (kind == ConnectorKind::Mssql || kind == ConnectorKind::Cockroach) && !schema.views.is_empty() {
        // cockroach/mssql views limited
        if kind == ConnectorKind::Cockroach {
            return Err(ConnectorError("CockroachDB views not supported in P2".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cockroach_capabilities() {
        let caps = ConnectorKind::Cockroach.capabilities();
        assert!(caps.transactions);
        assert!(!caps.enums);
    }
    #[test]
    fn unsupported_connector_error() {
        assert!(ConnectorKind::parse("oracle").is_err());
    }
}
