//! Explicit typed relation loading descriptors.
use std::marker::PhantomData;

/// Descriptor of one generated 1–N relationship. It contains identifiers, not data.
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
pub struct Many<P, C, K> {
    pub(crate) parent_table: &'static str,
    pub(crate) parent_key: &'static str,
    pub(crate) child_table: &'static str,
    pub(crate) child_key: &'static str,
    pub(crate) child_columns: &'static [&'static str],
    pub(crate) nullable_fk: bool,
    pub(crate) unique_child_fields: &'static [&'static str],
    pub(crate) composite_child_key: &'static [&'static str],
    pub(crate) through: Option<(&'static str, &'static str, &'static str)>,
    pub(crate) marker: PhantomData<fn() -> (P, C, K)>,
}
impl<P, C, K> Many<P, C, K> {
    /// Declare that the owning foreign key is nullable (normally generated from the schema).
    #[doc(hidden)]
    pub const fn with_nullable_fk(mut self) -> Self {
        self.nullable_fk = true;
        self
    }
    /// Unique scalar keys of the owning model; generated from the schema.
    #[doc(hidden)]
    pub const fn with_unique_child_fields(mut self, fields: &'static [&'static str]) -> Self {
        self.unique_child_fields = fields;
        self
    }
    #[doc(hidden)]
    pub const fn with_composite_child_key(mut self, key: &'static [&'static str]) -> Self {
        self.composite_child_key = key;
        self
    }
    pub fn some(self, filter: crate::query::Filter) -> crate::query::Filter {
        crate::query::Filter::relation(
            self.parent_table,
            self.child_table,
            self.parent_key,
            self.child_key,
            crate::query::RelationQuantifier::Some,
            filter,
        )
    }
    pub fn none(self, filter: crate::query::Filter) -> crate::query::Filter {
        crate::query::Filter::relation(
            self.parent_table,
            self.child_table,
            self.parent_key,
            self.child_key,
            crate::query::RelationQuantifier::None,
            filter,
        )
    }
    pub fn every(self, filter: crate::query::Filter) -> crate::query::Filter {
        crate::query::Filter::relation(
            self.parent_table,
            self.child_table,
            self.parent_key,
            self.child_key,
            crate::query::RelationQuantifier::Every,
            filter,
        )
    }
    #[doc(hidden)]
    pub const fn new(
        parent_table: &'static str,
        parent_key: &'static str,
        child_table: &'static str,
        child_key: &'static str,
        child_columns: &'static [&'static str],
    ) -> Self {
        Self {
            parent_table,
            parent_key,
            child_table,
            child_key,
            child_columns,
            nullable_fk: false,
            unique_child_fields: &[],
            composite_child_key: &[],
            through: None,
            marker: PhantomData,
        }
    }
}
/// Non-owning, optional side of a unique 1–1 relation.
#[derive(Debug)]
pub struct InverseOne<P, C, K>(Many<P, C, K>);
impl<P, C, K> InverseOne<P, C, K> {
    #[doc(hidden)]
    pub const fn new(
        parent_table: &'static str,
        parent_key: &'static str,
        child_table: &'static str,
        child_key: &'static str,
        child_columns: &'static [&'static str],
    ) -> Self {
        Self(Many::new(
            parent_table,
            parent_key,
            child_table,
            child_key,
            child_columns,
        ))
    }
    /// Declare that the owning foreign key is nullable (normally generated from the schema).
    #[doc(hidden)]
    pub const fn with_nullable_fk(mut self) -> Self {
        self.0.nullable_fk = true;
        self
    }
    #[doc(hidden)]
    pub const fn with_unique_child_fields(mut self, fields: &'static [&'static str]) -> Self {
        self.0.unique_child_fields = fields;
        self
    }
    #[doc(hidden)]
    pub const fn with_composite_child_key(mut self, key: &'static [&'static str]) -> Self {
        self.0.composite_child_key = key;
        self
    }
    /// True if a related child exists and matches the predicate.
    pub fn is(self, filter: crate::query::Filter) -> crate::query::Filter {
        self.0.some(filter)
    }
    /// True if no related child matches, including when the relation is absent.
    pub fn is_not(self, filter: crate::query::Filter) -> crate::query::Filter {
        self.0.none(filter)
    }
}
/// An implicit many-to-many relationship through a generated join table.
#[derive(Debug)]
pub struct Through<P, C, K, L>(Many<P, C, K>, PhantomData<fn() -> L>);
impl<P, C, K, L> Through<P, C, K, L> {
    #[doc(hidden)]
    pub const fn new(
        parent_table: &'static str,
        parent_key: &'static str,
        child_table: &'static str,
        child_key: &'static str,
        link_table: &'static str,
        link_parent: &'static str,
        link_child: &'static str,
        child_columns: &'static [&'static str],
    ) -> Self {
        Self(
            Many {
                parent_table,
                parent_key,
                child_table,
                child_key,
                child_columns,
                nullable_fk: false,
                unique_child_fields: &[],
                composite_child_key: &[],
                through: Some((link_table, link_parent, link_child)),
                marker: PhantomData,
            },
            PhantomData,
        )
    }
    fn filter(
        self,
        quantifier: crate::query::RelationQuantifier,
        filter: crate::query::Filter,
    ) -> crate::query::Filter {
        crate::query::Filter::relation_through(
            self.0.parent_table,
            self.0.child_table,
            self.0.parent_key,
            self.0.child_key,
            self.0.through.expect("through descriptor"),
            quantifier,
            filter,
        )
    }
    pub fn some(self, filter: crate::query::Filter) -> crate::query::Filter {
        self.filter(crate::query::RelationQuantifier::Some, filter)
    }
    pub fn none(self, filter: crate::query::Filter) -> crate::query::Filter {
        self.filter(crate::query::RelationQuantifier::None, filter)
    }
    pub fn every(self, filter: crate::query::Filter) -> crate::query::Filter {
        self.filter(crate::query::RelationQuantifier::Every, filter)
    }
}
/// Validated plan for creating one parent and one or more FK-owning children.
/// Build the entire plan before opening a transaction; relation keys are injected automatically.
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
pub struct NestedCreate<P, C, K> {
    pub(crate) parent: crate::query::CompiledQuery,
    children: Vec<(crate::query::CompiledQuery, usize)>,
    pub(crate) parent_key: &'static str,
    marker: PhantomData<fn() -> (P, C, K)>,
}
impl<P, C, K> NestedCreate<P, C, K> {
    /// Maximum number of child INSERT statements in one plan.
    pub const MAX_CHILDREN: usize = 1000;
    /// Create a parent and one or more 1–N children. Do not set the child FK yourself.
    pub fn many(
        parent: crate::query::Insert,
        relation: Many<P, C, K>,
        children: Vec<crate::query::Insert>,
    ) -> Result<Self, crate::query::QueryError> {
        if children.is_empty() {
            return Err(crate::query::QueryError::EmptyNestedWrite);
        }
        if children.len() > Self::MAX_CHILDREN {
            return Err(crate::query::QueryError::TooManyNestedWrites);
        }
        let parent = parent.nested_parent(relation.parent_table, relation.parent_key)?;
        let children = children
            .into_iter()
            .map(|child| {
                let query = child.nested_child(relation.child_table, relation.child_key)?;
                let slot = query.params.len() - 1;
                Ok((query, slot))
            })
            .collect::<Result<_, crate::query::QueryError>>()?;
        Ok(Self {
            parent,
            children,
            parent_key: relation.parent_key,
            marker: PhantomData,
        })
    }
    /// Create a parent and connect it to existing records in an implicit N–N relation.
    /// Keys must be distinct; missing targets cause the transaction to fail.
    pub fn connect<L>(
        parent: crate::query::Insert,
        relation: Through<P, C, K, L>,
        targets: Vec<L>,
    ) -> Result<Self, crate::query::QueryError>
    where
        L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq,
    {
        use crate::query::{CompiledQuery, QueryError, Value};
        if targets.is_empty() {
            return Err(QueryError::EmptyNestedWrite);
        }
        if targets.len() > Self::MAX_CHILDREN {
            return Err(QueryError::TooManyNestedWrites);
        }
        let mut unique = std::collections::HashSet::new();
        for target in &targets {
            if !unique.insert(target) {
                return Err(QueryError::DuplicateNestedTarget);
            }
        }
        let parent_key = relation.0.parent_key;
        let parent = parent.nested_parent(relation.0.parent_table, parent_key)?;
        let (table, local, remote) = relation.0.through.expect("through descriptor");
        let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let sql = format!(
            "INSERT INTO {} ({}, {}) VALUES ($1, $2)",
            quote(table),
            quote(local),
            quote(remote)
        );
        let children = targets
            .into_iter()
            .map(|key| {
                let value = crate::query::IntoFieldValue::into_value(key);
                if matches!(value, Value::Null) {
                    return Err(QueryError::NullInValueList);
                }
                Ok((
                    CompiledQuery {
                        sql: sql.clone(),
                        params: vec![Value::Null, value],
                    },
                    0,
                ))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            parent,
            children,
            parent_key,
            marker: PhantomData,
        })
    }
    /// Create a parent and its single optional inverse 1–1 child.
    pub fn one(
        parent: crate::query::Insert,
        relation: InverseOne<P, C, K>,
        child: crate::query::Insert,
    ) -> Result<Self, crate::query::QueryError> {
        Self::many(parent, relation.0, vec![child])
    }
}
#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_create<P, C, K>(
    tx: &mut crate::postgres::Transaction<'_>,
    plan: NestedCreate<P, C, K>,
) -> Result<K, crate::postgres::Error>
where
    K: crate::row::FromValue + crate::query::IntoFieldValue<K> + Clone,
{
    use crate::postgres::Error;
    let rows = tx.query(&plan.parent)?;
    if rows.len() != 1 {
        return Err(Error::UnexpectedResult);
    }
    let key: K = rows[0].get(plan.parent_key)?;
    let value = crate::query::IntoFieldValue::into_value(key.clone());
    if matches!(value, crate::query::Value::Null) {
        return Err(Error::InvalidInput("nested parent key cannot be null"));
    }
    for (mut child, slot) in plan.children {
        *child.params.get_mut(slot).ok_or(Error::UnexpectedResult)? = value.clone();
        if tx.execute(&child)? != 1 {
            return Err(Error::UnexpectedResult);
        }
    }
    Ok(key)
}
/// A bounded replacement of links in an optional 1–N or implicit N–N relation.
/// An empty collection requires an explicit clear constructor.
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
pub struct NestedSet<P, C, K, L> {
    relation: NestedSetRelation<P, C, K, L>,
    parent: crate::query::Value,
    targets: Vec<crate::query::Value>,
}
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
enum NestedSetRelation<P, C, K, L> {
    Through(Through<P, C, K, L>),
    Many(Many<P, C, K>, &'static str, PhantomData<fn() -> L>),
}
impl<P, C, K, L> NestedSet<P, C, K, L> {
    pub const MAX_TARGETS: usize = 1000;
    pub fn through(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        targets: Vec<L>,
    ) -> Result<Self, crate::query::QueryError>
    where
        L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq,
    {
        use crate::query::{QueryError, Value};
        if targets.is_empty() {
            return Err(QueryError::EmptyNestedWrite);
        }
        if targets.len() > Self::MAX_TARGETS {
            return Err(QueryError::TooManyNestedWrites);
        }
        let mut seen = std::collections::HashSet::new();
        if targets.iter().any(|key| !seen.insert(key)) {
            return Err(QueryError::DuplicateNestedTarget);
        }
        let parent = parent.into_value();
        let values: Vec<_> = targets
            .into_iter()
            .map(crate::query::IntoFieldValue::into_value)
            .collect();
        if matches!(parent, Value::Null) || values.iter().any(|v| matches!(v, Value::Null)) {
            return Err(QueryError::NullInValueList);
        }
        Ok(Self {
            relation: NestedSetRelation::Through(relation),
            parent,
            targets: values,
        })
    }
    pub fn clear_through(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
    ) -> Result<Self, crate::query::QueryError> {
        let parent = parent.into_value();
        if matches!(parent, crate::query::Value::Null) {
            return Err(crate::query::QueryError::NullInValueList);
        }
        Ok(Self {
            relation: NestedSetRelation::Through(relation),
            parent,
            targets: vec![],
        })
    }
    /// Replace a 1–N collection; children linked to another parent are refused.
    pub fn many(
        relation: Many<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        key: crate::query::Field<L>,
        targets: Vec<L>,
    ) -> Result<Self, crate::query::QueryError>
    where
        L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq,
    {
        use crate::query::{QueryError, Value};
        if !relation.nullable_fk {
            return Err(QueryError::RequiredRelationViolation);
        }
        if key.table_name() != relation.child_table {
            return Err(QueryError::WrongModel);
        }
        if !relation.unique_child_fields.contains(&key.column_name()) {
            return Err(QueryError::NonUniqueNestedSelector);
        }
        if targets.is_empty() {
            return Err(QueryError::EmptyNestedWrite);
        }
        if targets.len() > Self::MAX_TARGETS {
            return Err(QueryError::TooManyNestedWrites);
        }
        let mut unique = std::collections::HashSet::new();
        if targets.iter().any(|v| !unique.insert(v)) {
            return Err(QueryError::DuplicateNestedTarget);
        }
        let parent = parent.into_value();
        let targets: Vec<_> = targets
            .into_iter()
            .map(crate::query::IntoFieldValue::into_value)
            .collect();
        if matches!(parent, Value::Null) || targets.iter().any(|v| matches!(v, Value::Null)) {
            return Err(QueryError::NullInValueList);
        }
        Ok(Self {
            relation: NestedSetRelation::Many(relation, key.column_name(), PhantomData),
            parent,
            targets,
        })
    }
    /// Explicitly clear an optional 1–N collection without deleting children.
    pub fn clear_many(
        relation: Many<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        key: crate::query::Field<L>,
    ) -> Result<Self, crate::query::QueryError> {
        use crate::query::{QueryError, Value};
        if !relation.nullable_fk {
            return Err(QueryError::RequiredRelationViolation);
        }
        if key.table_name() != relation.child_table {
            return Err(QueryError::WrongModel);
        }
        if !relation.unique_child_fields.contains(&key.column_name()) {
            return Err(QueryError::NonUniqueNestedSelector);
        }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) {
            return Err(QueryError::NullInValueList);
        }
        Ok(Self {
            relation: NestedSetRelation::Many(relation, key.column_name(), PhantomData),
            parent,
            targets: vec![],
        })
    }
}
#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_set<P, C, K, L>(
    tx: &mut crate::postgres::Transaction<'_>,
    plan: NestedSet<P, C, K, L>,
) -> Result<(), crate::postgres::Error>
where
    L: crate::row::FromValue + std::hash::Hash + Eq,
{
    use crate::postgres::Error;
    use crate::query::{CompiledQuery, Value};
    let relation = match plan.relation {
        NestedSetRelation::Through(relation) => relation.0,
        NestedSetRelation::Many(relation, key, _) => {
            return execute_nested_set_many::<P, C, K, L>(
                tx,
                relation,
                key,
                plan.parent,
                plan.targets,
            )
        }
    };
    let (link, local, remote) = relation.through.expect("through descriptor");
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    // Block DDL, including changes to FK constraints and link triggers, while inspecting and writing.
    tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(link)))?;
    tx.execute_statement(&format!(
        "LOCK TABLE {}, {} IN ROW SHARE MODE",
        quote(relation.parent_table),
        quote(relation.child_table)
    ))?;
    let inspection = CompiledQuery {
        sql: r#"WITH RECURSIVE rels(oid) AS (SELECT to_regclass($1)::oid UNION SELECT i.inhrelid FROM pg_catalog.pg_inherits i JOIN rels r ON i.inhparent=r.oid)
SELECT current_setting('transaction_isolation') AS isolation,
 row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) OR row_security_active(to_regclass($3)) AS rls,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c WHERE c.contype='f' AND c.confrelid=to_regclass($1)) AS referenced_link,
 (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t JOIN rels r ON r.oid=t.tgrelid WHERE NOT t.tgisinternal AND t.tgenabled <> 'D') OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w JOIN rels r ON r.oid=w.ev_class WHERE w.rulename <> '_RETURN')) AS unsafe_rewriter,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1] JOIN pg_catalog.pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=c.confkey[1] WHERE c.contype='f' AND c.conrelid=to_regclass($1) AND c.confrelid=to_regclass($2) AND array_length(c.conkey,1)=1 AND array_length(c.confkey,1)=1 AND a.attname=$4 AND b.attname=$5 AND c.convalidated AND NOT c.condeferrable) AS parent_fk,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1] JOIN pg_catalog.pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=c.confkey[1] WHERE c.contype='f' AND c.conrelid=to_regclass($1) AND c.confrelid=to_regclass($3) AND array_length(c.conkey,1)=1 AND array_length(c.confkey,1)=1 AND a.attname=$6 AND b.attname=$7 AND c.convalidated AND NOT c.condeferrable) AS child_fk"#.to_owned(),
        params: vec![Value::String(quote(link)), Value::String(quote(relation.parent_table)), Value::String(quote(relation.child_table)), Value::String(local.into()), Value::String(relation.parent_key.into()), Value::String(remote.into()), Value::String(relation.child_key.into())],
    };
    let rows = tx.query(&inspection)?;
    if rows.len() != 1 {
        return Err(Error::UnexpectedResult);
    }
    if rows[0].get::<String>("isolation")? != "read committed"
        || rows[0].get::<bool>("rls")?
        || rows[0].get::<bool>("unsafe_rewriter")?
        || rows[0].get::<bool>("referenced_link")?
        || !rows[0].get::<bool>("parent_fk")?
        || !rows[0].get::<bool>("child_fk")?
    {
        return Err(Error::InvalidInput("nested set requires READ COMMITTED, full visibility, immediate validated FKs, and no external references, user trigger or rewrite on the join table"));
    }
    let query = |sql: String, params: Vec<Value>| CompiledQuery { sql, params };
    let parent = query(
        format!(
            "SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE",
            quote(relation.parent_table),
            quote(relation.parent_key)
        ),
        vec![plan.parent.clone()],
    );
    match tx.query(&parent)?.len() {
        1 => (),
        0 => return Err(Error::RelationNotFound),
        _ => return Err(Error::RelationAmbiguous),
    }
    let mut seen = std::collections::HashSet::new();
    for key in &plan.targets {
        let target = query(
            format!(
                "SELECT {} AS __rustorm_key FROM {} WHERE {} = $1 FOR KEY SHARE",
                quote(relation.child_key),
                quote(relation.child_table),
                quote(relation.child_key)
            ),
            vec![key.clone()],
        );
        let rows = tx.query(&target)?;
        if rows.is_empty() {
            return Err(Error::RelationNotFound);
        }
        if rows.len() != 1 {
            return Err(Error::RelationAmbiguous);
        }
        let canonical: L = rows[0].get("__rustorm_key")?;
        if !seen.insert(canonical) {
            return Err(Error::InvalidInput(
                "nested set contains duplicate database keys",
            ));
        }
    }
    let locked = query(
        format!(
            "SELECT 1 FROM {} WHERE {} = $1 LIMIT 1001 FOR UPDATE",
            quote(link),
            quote(local)
        ),
        vec![plan.parent.clone()],
    );
    let existing = tx.query(&locked)?.len();
    if existing > NestedSet::<P, C, K, L>::MAX_TARGETS {
        return Err(Error::InvalidInput(
            "nested set exceeds existing-link limit",
        ));
    }
    let remove = query(
        format!("DELETE FROM {} WHERE {} = $1", quote(link), quote(local)),
        vec![plan.parent.clone()],
    );
    if tx.execute(&remove)? != existing as u64 {
        return Err(Error::UnexpectedResult);
    }
    let sql = format!(
        "INSERT INTO {} ({}, {}) VALUES ($1, $2)",
        quote(link),
        quote(local),
        quote(remote)
    );
    for key in plan.targets {
        if tx.execute(&query(sql.clone(), vec![plan.parent.clone(), key]))? != 1 {
            return Err(Error::UnexpectedResult);
        }
    }
    Ok(())
}

#[cfg(feature = "postgres")]
fn execute_nested_set_many<P, C, K, L>(
    tx: &mut crate::postgres::Transaction<'_>,
    relation: Many<P, C, K>,
    key: &'static str,
    parent: crate::query::Value,
    targets: Vec<crate::query::Value>,
) -> Result<(), crate::postgres::Error>
where
    L: crate::row::FromValue + std::hash::Hash + Eq,
{
    use crate::postgres::Error;
    use crate::query::{CompiledQuery, Value};
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    tx.execute_statement(&format!(
        "LOCK TABLE {} IN ROW EXCLUSIVE MODE",
        quote(relation.child_table)
    ))?;
    tx.execute_statement(&format!(
        "LOCK TABLE {} IN ROW SHARE MODE",
        quote(relation.parent_table)
    ))?;
    let inspection = CompiledQuery {
        sql: r#"WITH RECURSIVE rels(oid) AS (SELECT to_regclass($1)::oid UNION SELECT i.inhrelid FROM pg_catalog.pg_inherits i JOIN rels r ON i.inhparent=r.oid)
SELECT current_setting('transaction_isolation') AS isolation,
 row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) AS rls,
 EXISTS (SELECT 1 FROM pg_catalog.pg_inherits i WHERE i.inhparent=to_regclass($1) OR i.inhparent=to_regclass($2)) AS inherited,
 (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t JOIN rels r ON r.oid=t.tgrelid WHERE NOT t.tgisinternal AND t.tgenabled <> 'D') OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w JOIN rels r ON r.oid=w.ev_class WHERE w.rulename <> '_RETURN')) AS unsafe_rewriter,
 EXISTS (SELECT 1 FROM pg_catalog.pg_attribute a WHERE a.attrelid=to_regclass($1) AND a.attname=$3 AND NOT a.attnotnull AND NOT a.attisdropped) AS nullable_fk,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1] JOIN pg_catalog.pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=c.confkey[1] WHERE c.contype='f' AND c.conrelid=to_regclass($1) AND c.confrelid=to_regclass($2) AND array_length(c.conkey,1)=1 AND array_length(c.confkey,1)=1 AND a.attname=$3 AND b.attname=$4 AND c.convalidated AND NOT c.condeferrable) AS parent_fk,
 EXISTS (SELECT 1 FROM pg_catalog.pg_index i JOIN pg_catalog.pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=i.indkey[0] WHERE i.indrelid=to_regclass($1) AND a.attname=$5 AND i.indisunique AND i.indisvalid AND i.indisready AND i.indimmediate AND i.indnkeyatts=1 AND i.indpred IS NULL AND i.indexprs IS NULL) AS unique_key,
 EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.confrelid AND a.attnum=ANY(c.confkey) WHERE c.contype='f' AND c.confrelid=to_regclass($1) AND a.attname=$3) AS referenced_fk"#.to_owned(),
        params: vec![Value::String(quote(relation.child_table)),Value::String(quote(relation.parent_table)),Value::String(relation.child_key.into()),Value::String(relation.parent_key.into()),Value::String(key.into())],
    };
    let rows = tx.query(&inspection)?;
    if rows.len() != 1 {
        return Err(Error::UnexpectedResult);
    }
    if rows[0].get::<String>("isolation")? != "read committed"
        || rows[0].get::<bool>("rls")?
        || rows[0].get::<bool>("inherited")?
        || rows[0].get::<bool>("unsafe_rewriter")?
        || !rows[0].get::<bool>("nullable_fk")?
        || !rows[0].get::<bool>("parent_fk")?
        || !rows[0].get::<bool>("unique_key")?
        || rows[0].get::<bool>("referenced_fk")?
    {
        return Err(Error::InvalidInput("nested 1–N set requires READ COMMITTED, nullable immediate FK, a unique target key, full visibility, no inheritance, external FK references, triggers or rules"));
    }
    let query = |sql: String, params: Vec<Value>| CompiledQuery { sql, params };
    let parent_query = query(
        format!(
            "SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE",
            quote(relation.parent_table),
            quote(relation.parent_key)
        ),
        vec![parent.clone()],
    );
    match tx.query(&parent_query)?.len() {
        1 => (),
        0 => return Err(Error::RelationNotFound),
        _ => return Err(Error::RelationAmbiguous),
    }
    let mut seen = std::collections::HashSet::new();
    for target in &targets {
        let select=query(format!("SELECT {} AS __rustorm_key, ({} IS NULL OR {} = $1) AS available FROM {} WHERE {} = $2 FOR UPDATE",quote(key),quote(relation.child_key),quote(relation.child_key),quote(relation.child_table),quote(key)),vec![parent.clone(),target.clone()]);
        let rows = tx.query(&select)?;
        if rows.is_empty() {
            return Err(Error::RelationNotFound);
        }
        if rows.len() != 1 {
            return Err(Error::RelationAmbiguous);
        }
        if !rows[0].get::<bool>("available")? {
            return Err(Error::RelationConflict);
        }
        let canonical: L = rows[0].get("__rustorm_key")?;
        if !seen.insert(canonical) {
            return Err(Error::InvalidInput(
                "nested set contains duplicate database keys",
            ));
        }
    }
    let locked = query(
        format!(
            "SELECT 1 FROM {} WHERE {} = $1 LIMIT 1001 FOR UPDATE",
            quote(relation.child_table),
            quote(relation.child_key)
        ),
        vec![parent.clone()],
    );
    let existing = tx.query(&locked)?.len();
    if existing > NestedSet::<P, C, K, L>::MAX_TARGETS {
        return Err(Error::InvalidInput(
            "nested set exceeds existing-link limit",
        ));
    }
    let detach = query(
        format!(
            "UPDATE {} SET {} = NULL WHERE {} = $1",
            quote(relation.child_table),
            quote(relation.child_key),
            quote(relation.child_key)
        ),
        vec![parent.clone()],
    );
    if tx.execute(&detach)? != existing as u64 {
        return Err(Error::UnexpectedResult);
    }
    let sql = format!(
        "UPDATE {} SET {} = $1 WHERE {} = $2 AND {} IS NULL",
        quote(relation.child_table),
        quote(relation.child_key),
        quote(key),
        quote(relation.child_key)
    );
    for target in targets {
        if tx.execute(&query(sql.clone(), vec![parent.clone(), target]))? != 1 {
            return Err(Error::UnexpectedResult);
        }
    }
    Ok(())
}

/// Validated scoped update or deletion of exactly one child in a 1–N relation.
/// The supplied filter is checked at execution: zero or multiple matches roll back.
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
struct ExclusiveTarget {
    settings: crate::query::CompiledQuery,
    target_lock: crate::query::CompiledQuery,
    other_links: crate::query::CompiledQuery,
}
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
pub struct NestedChange<P, C, K> {
    query: crate::query::CompiledQuery,
    verify_link: Option<crate::query::CompiledQuery>,
    precondition_link: Option<crate::query::CompiledQuery>,
    exclusive_target: Option<ExclusiveTarget>,
    guard_disconnect_table: Option<&'static str>,
    marker: PhantomData<fn() -> (P, C, K)>,
}
impl<P, C, K> NestedChange<P, C, K> {
    /// Connect two existing records, idempotently; both endpoints are protected by database FKs.
    pub fn connect_through<L>(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<Self, crate::query::QueryError> {
        let (table, local, remote) = relation.0.through.expect("through descriptor");
        let quote = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
        let query = through_write(relation, parent, child, true);
        if query
            .params
            .iter()
            .any(|v| matches!(v, crate::query::Value::Null))
        {
            return Err(crate::query::QueryError::NullInValueList);
        }
        let verify_link = crate::query::CompiledQuery {
            sql: format!(
                "SELECT 1 FROM {} WHERE {} = $1 AND {} = $2 FOR KEY SHARE",
                quote(table),
                quote(local),
                quote(remote)
            ),
            params: query.params.clone(),
        };
        Ok(Self {
            query,
            verify_link: Some(verify_link),
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
    /// Disconnect exactly one N–N pair without deleting either endpoint.
    pub fn disconnect_through<L>(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
    ) -> Result<Self, crate::query::QueryError> {
        let query = through_write(relation, parent, child, false);
        if query
            .params
            .iter()
            .any(|v| matches!(v, crate::query::Value::Null))
        {
            return Err(crate::query::QueryError::NullInValueList);
        }
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
    /// Update a linked N–N target. The target may be shared; all its readers observe the update.
    /// Its key cannot be changed. The link is locked until this transaction finishes.
    pub fn update_through<L>(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
        update: crate::query::Update,
    ) -> Result<Self, crate::query::QueryError> {
        let parent = parent.into_value();
        let child = child.into_value();
        if matches!(parent, crate::query::Value::Null) || matches!(child, crate::query::Value::Null)
        {
            return Err(crate::query::QueryError::NullInValueList);
        }
        let (table, local, remote) = relation.0.through.expect("through descriptor");
        let quote = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
        let precondition_link = crate::query::CompiledQuery {
            sql: format!(
                "SELECT 1 FROM {} WHERE {} = $1 AND {} = $2 FOR KEY SHARE",
                quote(table),
                quote(local),
                quote(remote)
            ),
            params: vec![parent, child.clone()],
        };
        let query = update.nested_target(relation.0.child_table, relation.0.child_key, child)?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: Some(precondition_link),
            exclusive_target: None,
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
    /// Delete a linked N–N target only when no other parent links to it.
    /// Requires READ COMMITTED and full visibility of the join table (no active RLS).
    pub fn delete_through<L>(
        relation: Through<P, C, K, L>,
        parent: impl crate::query::IntoFieldValue<K>,
        child: impl crate::query::IntoFieldValue<L>,
        delete: crate::query::Delete,
    ) -> Result<Self, crate::query::QueryError> {
        use crate::query::{CompiledQuery, Value};
        let parent = parent.into_value();
        let child = child.into_value();
        if matches!(parent, Value::Null) || matches!(child, Value::Null) {
            return Err(crate::query::QueryError::NullInValueList);
        }
        let (table, local, remote) = relation.0.through.expect("through descriptor");
        let quote = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
        let params = vec![parent, child.clone()];
        let precondition_link = CompiledQuery {
            sql: format!(
                "SELECT 1 FROM {} WHERE {} = $1 AND {} = $2 FOR KEY SHARE",
                quote(table),
                quote(local),
                quote(remote)
            ),
            params: params.clone(),
        };
        let guard = ExclusiveTarget {
            settings: CompiledQuery {
                sql: "SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) AS rls".to_owned(),
                params: vec![Value::String(quote(table))],
            },
            target_lock: CompiledQuery {
                sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(relation.0.child_table), quote(relation.0.child_key)),
                params: vec![child.clone()],
            },
            other_links: CompiledQuery {
                sql: format!("SELECT 1 FROM {} WHERE {} = $2 AND {} <> $1 LIMIT 1", quote(table), quote(remote), quote(local)),
                params,
            },
        };
        let query = delete.nested_target(relation.0.child_table, relation.0.child_key, child)?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: Some(precondition_link),
            exclusive_target: Some(guard),
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
    /// Detach exactly one child by setting its nullable FK to NULL; the child is preserved.
    pub fn disconnect(
        relation: Many<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
    ) -> Result<Self, crate::query::QueryError> {
        if !relation.nullable_fk {
            return Err(crate::query::QueryError::RequiredRelationViolation);
        }
        let query = crate::query::Update::nested_disconnect(
            relation.child_table,
            relation.child_key,
            parent.into_value(),
            filter,
            relation.unique_child_fields,
            relation.composite_child_key,
        )?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: Some(relation.child_table),
            marker: PhantomData,
        })
    }
    /// Detach from the inverse side of an optional 1–1 relation.
    pub fn disconnect_one(
        relation: InverseOne<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
    ) -> Result<Self, crate::query::QueryError> {
        Self::disconnect(relation.0, parent, filter)
    }
    /// Detach from the FK-owning side of an optional 1–N or 1–1 relation.
    pub fn disconnect_owner(
        relation: One<C, P, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
    ) -> Result<Self, crate::query::QueryError> {
        if !relation.nullable {
            return Err(crate::query::QueryError::RequiredRelationViolation);
        }
        let query = crate::query::Update::nested_disconnect(
            relation.child_table,
            relation.child_key,
            parent.into_value(),
            filter,
            relation.unique_child_fields,
            relation.composite_child_key,
        )?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: Some(relation.child_table),
            marker: PhantomData,
        })
    }
    /// Update one linked child without changing its owning foreign key.
    pub fn update(
        relation: Many<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
        update: crate::query::Update,
    ) -> Result<Self, crate::query::QueryError> {
        let query = update.nested_scoped(
            relation.child_table,
            relation.child_key,
            parent.into_value(),
            filter,
        )?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
    /// Update the related record from the inverse side of a 1–1 relation.
    pub fn update_one(
        relation: InverseOne<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
        update: crate::query::Update,
    ) -> Result<Self, crate::query::QueryError> {
        Self::update(relation.0, parent, filter, update)
    }
    /// Delete the related record from the inverse side of a 1–1 relation.
    pub fn delete_one(
        relation: InverseOne<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
        delete: crate::query::Delete,
    ) -> Result<Self, crate::query::QueryError> {
        Self::delete(relation.0, parent, filter, delete)
    }
    /// Delete one linked child. Database FK cascade rules still apply.
    pub fn delete(
        relation: Many<P, C, K>,
        parent: impl crate::query::IntoFieldValue<K>,
        filter: crate::query::Filter,
        delete: crate::query::Delete,
    ) -> Result<Self, crate::query::QueryError> {
        let query = delete.nested_scoped(
            relation.child_table,
            relation.child_key,
            parent.into_value(),
            filter,
        )?;
        Ok(Self {
            query,
            verify_link: None,
            precondition_link: None,
            exclusive_target: None,
            guard_disconnect_table: None,
            marker: PhantomData,
        })
    }
}
#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_change<P, C, K>(
    tx: &mut crate::postgres::Transaction<'_>,
    plan: NestedChange<P, C, K>,
) -> Result<(), crate::postgres::Error> {
    if let Some(table) = plan.guard_disconnect_table {
        // Prevent a concurrent CREATE/ALTER TRIGGER or RULE between inspection and UPDATE.
        // READ COMMITTED is required so inspection observes DDL committed before the lock.
        let quote = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
        tx.execute_statement(&format!(
            "LOCK TABLE {} IN ROW EXCLUSIVE MODE",
            quote(table)
        ))?;
        let inspection = crate::query::CompiledQuery {
            sql: "WITH RECURSIVE rels(oid) AS (SELECT to_regclass($1)::oid UNION SELECT i.inhrelid FROM pg_catalog.pg_inherits i JOIN rels r ON i.inhparent = r.oid) SELECT current_setting('transaction_isolation') AS isolation, (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t JOIN rels r ON r.oid = t.tgrelid WHERE NOT t.tgisinternal AND t.tgenabled <> 'D') OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w JOIN rels r ON r.oid = w.ev_class WHERE w.rulename <> '_RETURN')) AS unsafe_rewriter".to_owned(),
            params: vec![crate::query::Value::String(quote(table))],
        };
        let rows = tx.query(&inspection)?;
        if rows.len() != 1 {
            return Err(crate::postgres::Error::UnexpectedResult);
        }
        let isolation: String = rows[0].get("isolation")?;
        let unsafe_rewriter: bool = rows[0].get("unsafe_rewriter")?;
        if isolation != "read committed" || unsafe_rewriter {
            return Err(crate::postgres::Error::InvalidInput("disconnect requires READ COMMITTED and no enabled user trigger or rewrite rule on the FK table"));
        }
    }
    if let Some(guard) = &plan.exclusive_target {
        let settings = tx.query(&guard.settings)?;
        if settings.len() != 1 {
            return Err(crate::postgres::Error::UnexpectedResult);
        }
        let isolation: String = settings[0].get("isolation")?;
        let rls: bool = settings[0].get("rls")?;
        if isolation != "read committed" || rls {
            return Err(crate::postgres::Error::InvalidInput(
                "nested N–N delete requires READ COMMITTED and full join-table visibility",
            ));
        }
        match tx.query(&guard.target_lock)?.len() {
            1 => (),
            0 => return Err(crate::postgres::Error::RelationNotFound),
            _ => return Err(crate::postgres::Error::RelationAmbiguous),
        }
    }
    if let Some(precondition) = plan.precondition_link {
        match tx.query(&precondition)?.len() {
            1 => (),
            0 => return Err(crate::postgres::Error::RelationNotFound),
            _ => return Err(crate::postgres::Error::RelationAmbiguous),
        }
    }
    if let Some(guard) = &plan.exclusive_target {
        if !tx.query(&guard.other_links)?.is_empty() {
            return Err(crate::postgres::Error::RelationConflict);
        }
    }
    let affected = tx.execute(&plan.query)?;
    if let Some(verify) = plan.verify_link {
        if affected > 1 {
            return Err(crate::postgres::Error::RelationAmbiguous);
        }
        return match tx.query(&verify)?.len() {
            1 => Ok(()),
            0 => Err(crate::postgres::Error::RelationNotFound),
            _ => Err(crate::postgres::Error::RelationAmbiguous),
        };
    }
    match affected {
        1 => Ok(()),
        0 => Err(crate::postgres::Error::RelationNotFound),
        _ => Err(crate::postgres::Error::RelationAmbiguous),
    }
}
/// A parent with zero or one explicitly loaded inverse child.
#[derive(Debug, Clone, PartialEq)]
pub struct WithInverseOne<P, C> {
    pub parent: P,
    pub child: Option<C>,
}

#[cfg(feature = "postgres")]
pub(crate) fn include_inverse_one<P, C, K>(
    select: crate::query::Select,
    relation: InverseOne<P, C, K>,
    run: impl FnMut(
        &crate::query::CompiledQuery,
    ) -> Result<Vec<crate::row::Row>, crate::postgres::Error>,
) -> Result<Vec<WithInverseOne<P, C>>, crate::postgres::Error>
where
    P: crate::row::FromRow,
    C: crate::row::FromRow,
    K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
{
    include_many(select, relation.0, run)?
        .into_iter()
        .map(|entry| {
            if entry.children.len() > 1 {
                return Err(crate::postgres::Error::UnexpectedResult);
            }
            Ok(WithInverseOne {
                parent: entry.parent,
                child: entry.children.into_iter().next(),
            })
        })
        .collect()
}
#[cfg(feature = "postgres")]
pub(crate) fn include_through<P, C, K, L>(
    select: crate::query::Select,
    relation: Through<P, C, K, L>,
    run: impl FnMut(
        &crate::query::CompiledQuery,
    ) -> Result<Vec<crate::row::Row>, crate::postgres::Error>,
) -> Result<Vec<WithMany<P, C>>, crate::postgres::Error>
where
    P: crate::row::FromRow,
    C: crate::row::FromRow,
    K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
{
    include_many(select, relation.0, run)
}
pub(crate) fn through_write<P, C, K, L>(
    relation: Through<P, C, K, L>,
    parent: impl crate::query::IntoFieldValue<K>,
    child: impl crate::query::IntoFieldValue<L>,
    connect: bool,
) -> crate::query::CompiledQuery {
    let (table, local, remote) = relation.0.through.expect("through descriptor");
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let sql = if connect {
        format!(
            "INSERT INTO {} ({}, {}) VALUES ($1, $2) ON CONFLICT ({}, {}) DO NOTHING",
            quote(table),
            quote(local),
            quote(remote),
            quote(local),
            quote(remote)
        )
    } else {
        format!(
            "DELETE FROM {} WHERE {} = $1 AND {} = $2",
            quote(table),
            quote(local),
            quote(remote)
        )
    };
    crate::query::CompiledQuery {
        sql,
        params: vec![parent.into_value(), child.into_value()],
    }
}
/// A parent record and its explicitly loaded children. Child order is unspecified.
#[derive(Debug, Clone, PartialEq)]
pub struct WithMany<P, C> {
    pub parent: P,
    pub children: Vec<C>,
}

/// Descriptor for the owning side of a 1–N relation.
#[derive(Debug)]
#[cfg_attr(not(feature = "postgres"), allow(dead_code))]
pub struct One<C, P, K> {
    child_table: &'static str,
    child_key: &'static str,
    parent_table: &'static str,
    parent_key: &'static str,
    nullable: bool,
    parent_columns: &'static [&'static str],
    unique_child_fields: &'static [&'static str],
    composite_child_key: &'static [&'static str],
    marker: PhantomData<fn() -> (C, P, K)>,
}
impl<C, P, K> One<C, P, K> {
    #[doc(hidden)]
    pub const fn with_unique_child_fields(mut self, fields: &'static [&'static str]) -> Self {
        self.unique_child_fields = fields;
        self
    }
    #[doc(hidden)]
    pub const fn with_composite_child_key(mut self, key: &'static [&'static str]) -> Self {
        self.composite_child_key = key;
        self
    }
    /// True when the related parent exists and matches the predicate.
    pub fn is(self, filter: crate::query::Filter) -> crate::query::Filter {
        crate::query::Filter::relation(
            self.child_table,
            self.parent_table,
            self.child_key,
            self.parent_key,
            crate::query::RelationQuantifier::Some,
            filter,
        )
    }
    /// True when no related parent matches, including a null foreign key.
    pub fn is_not(self, filter: crate::query::Filter) -> crate::query::Filter {
        crate::query::Filter::relation(
            self.child_table,
            self.parent_table,
            self.child_key,
            self.parent_key,
            crate::query::RelationQuantifier::None,
            filter,
        )
    }
    #[doc(hidden)]
    pub const fn new(
        child_table: &'static str,
        child_key: &'static str,
        parent_table: &'static str,
        parent_key: &'static str,
        nullable: bool,
        parent_columns: &'static [&'static str],
    ) -> Self {
        Self {
            child_table,
            child_key,
            parent_table,
            parent_key,
            nullable,
            parent_columns,
            unique_child_fields: &[],
            composite_child_key: &[],
            marker: PhantomData,
        }
    }
}
/// A child and its loaded parent; None represents a NULL foreign key.
#[derive(Debug, Clone, PartialEq)]
pub struct WithOne<C, P> {
    pub child: C,
    pub parent: Option<P>,
}

#[cfg(feature = "postgres")]
pub(crate) fn include_one<C, P, K>(
    select: crate::query::Select,
    relation: One<C, P, K>,
    mut run: impl FnMut(
        &crate::query::CompiledQuery,
    ) -> Result<Vec<crate::row::Row>, crate::postgres::Error>,
) -> Result<Vec<WithOne<C, P>>, crate::postgres::Error>
where
    C: crate::row::FromRow,
    P: crate::row::FromRow,
    K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
{
    use crate::{postgres::Error, query::CompiledQuery};
    use std::collections::{HashMap, HashSet};
    if !select.supports_relation(relation.child_table, relation.child_key)
        || relation.parent_columns.is_empty()
        || !relation.parent_columns.contains(&relation.parent_key)
    {
        return Err(Error::InvalidInput(
            "relation requires the matching model and full selections",
        ));
    }
    let query = select
        .compile()
        .map_err(|_| Error::InvalidInput("invalid child selection"))?;
    let rows = run(&query)?;
    let mut children = Vec::with_capacity(rows.len());
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for row in &rows {
        let key: Option<K> = row.get(relation.child_key)?;
        if key.is_none() && !relation.nullable {
            return Err(Error::UnexpectedResult);
        }
        if let Some(k) = &key {
            if seen.insert(k.clone()) {
                keys.push(k.clone());
            }
        }
        children.push((C::from_row(row)?, key));
    }
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let alias = (0usize..)
        .map(|n| format!("__rustorm_input_{n}"))
        .find(|a| !relation.parent_columns.iter().any(|c| *c == a))
        .unwrap();
    let columns = relation
        .parent_columns
        .iter()
        .map(|c| format!("p.{}", quote(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut parents = HashMap::new();
    // Each bound input is returned as-is: a database collation may equate it to
    // a differently spelled stored key. Comparing Rust strings would be wrong.
    for chunk in keys.chunks(500) {
        let expected: HashSet<&K> = chunk.iter().collect();
        let sql = (1..=chunk.len())
            .map(|n| {
                format!(
                    "SELECT {columns}, ${n} AS {} FROM {} AS p WHERE p.{} = ${n}",
                    quote(&alias),
                    quote(relation.parent_table),
                    quote(relation.parent_key)
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        let params = chunk
            .iter()
            .cloned()
            .map(crate::query::IntoFieldValue::into_value)
            .collect();
        for row in run(&CompiledQuery { sql, params })? {
            let key: K = row.get(&alias)?;
            if !expected.contains(&key) || parents.insert(key, row).is_some() {
                return Err(Error::UnexpectedResult);
            }
        }
    }
    children
        .into_iter()
        .map(|(child, key)| {
            let parent = match key {
                None => None,
                Some(k) => Some(P::from_row(
                    parents.get(&k).ok_or(Error::UnexpectedResult)?,
                )?),
            };
            Ok(WithOne { child, parent })
        })
        .collect()
}

#[cfg(feature = "postgres")]
pub(crate) fn include_many<P, C, K>(
    select: crate::query::Select,
    relation: Many<P, C, K>,
    mut run: impl FnMut(
        &crate::query::CompiledQuery,
    ) -> Result<Vec<crate::row::Row>, crate::postgres::Error>,
) -> Result<Vec<WithMany<P, C>>, crate::postgres::Error>
where
    P: crate::row::FromRow,
    C: crate::row::FromRow,
    K: crate::row::FromValue + crate::query::IntoFieldValue<K> + std::hash::Hash + Eq + Clone,
{
    use crate::postgres::Error;
    use crate::query::CompiledQuery;
    use std::collections::{HashMap, HashSet};
    if !select.supports_relation(relation.parent_table, relation.parent_key) {
        return Err(Error::InvalidInput(
            "relation requires the matching model and an unprojected parent selection",
        ));
    }
    if relation.child_columns.is_empty() {
        return Err(Error::InvalidInput("relation child selection is empty"));
    }
    let parent_query = select
        .compile()
        .map_err(|_| Error::InvalidInput("invalid parent selection"))?;
    let parent_rows = run(&parent_query)?;
    let mut keys = Vec::with_capacity(parent_rows.len());
    let mut records = Vec::with_capacity(parent_rows.len());
    let mut seen = HashSet::new();
    for row in &parent_rows {
        let key: K = row.get(relation.parent_key)?;
        if !seen.insert(key.clone()) {
            return Err(Error::InvalidInput("relation parent key is not unique"));
        }
        keys.push(key);
        records.push(P::from_row(row)?);
    }
    if records.is_empty() {
        return Ok(Vec::new());
    }
    // Use a SQL join to retrieve the canonical stored parent key, rather than
    // comparing the child's FK bytes: collation can equate distinct text values.
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    // Keep the internal alias short even if many user columns resemble it.
    let mut suffix = 0usize;
    let alias = loop {
        let candidate = format!("__rustorm_parent_{suffix}");
        if !relation
            .child_columns
            .iter()
            .any(|column| *column == candidate)
        {
            break candidate;
        }
        suffix += 1;
    };
    let columns = relation
        .child_columns
        .iter()
        .map(|name| format!("c.{}", quote(name)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut grouped: HashMap<K, Vec<C>> = HashMap::new();
    for chunk in keys.chunks(500) {
        let params = chunk
            .iter()
            .cloned()
            .map(crate::query::IntoFieldValue::into_value)
            .collect::<Vec<_>>();
        let placeholders = (1..=params.len())
            .map(|n| format!("${n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let join = if let Some((table, local, remote)) = relation.through {
            format!(
                "JOIN {} AS l ON l.{} = p.{} JOIN {} AS c ON c.{} = l.{}",
                quote(table),
                quote(local),
                quote(relation.parent_key),
                quote(relation.child_table),
                quote(relation.child_key),
                quote(remote)
            )
        } else {
            format!(
                "JOIN {} AS c ON c.{} = p.{}",
                quote(relation.child_table),
                quote(relation.child_key),
                quote(relation.parent_key)
            )
        };
        let sql = format!(
            "SELECT {columns}, p.{} AS {} FROM {} AS p {join} WHERE p.{} IN ({placeholders})",
            quote(relation.parent_key),
            quote(&alias),
            quote(relation.parent_table),
            quote(relation.parent_key)
        );
        for row in run(&CompiledQuery { sql, params })? {
            let key: K = row.get(&alias)?;
            if !seen.contains(&key) {
                return Err(Error::UnexpectedResult);
            }
            grouped.entry(key).or_default().push(C::from_row(&row)?);
        }
    }
    Ok(records
        .into_iter()
        .zip(keys)
        .map(|(parent, key)| WithMany {
            parent,
            children: grouped.remove(&key).unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "postgres"))]
mod tests {
    use super::*;
    use crate::{
        query::Select,
        row::{Column, DecodeError, FromRow, Row},
    };
    #[derive(Debug)]
    struct Parent(i32);
    impl FromRow for Parent {
        fn from_row(row: &Row) -> Result<Self, DecodeError> {
            Ok(Self(row.get("id")?))
        }
    }
    #[derive(Debug)]
    struct Child;
    impl FromRow for Child {
        fn from_row(_: &Row) -> Result<Self, DecodeError> {
            Ok(Self)
        }
    }
    fn row(n: i32) -> Row {
        Row::new(vec![Column {
            name: "id".into(),
            oid: 23,
            value: Some(n.to_string()),
        }])
    }
    fn descriptor() -> Many<Parent, Child, i32> {
        Many::new("p", "id", "c", "p_id", &["id", "p_id"])
    }
    #[test]
    fn inverse_one_rejects_more_than_one_matching_child() {
        let relation =
            InverseOne::<Parent, Child, i32>::new("p", "id", "c", "p_id", &["id", "p_id"]);
        let mut calls = 0;
        let result = include_inverse_one(Select::new("p", &["id"]), relation, |_| {
            calls += 1;
            if calls == 1 {
                return Ok(vec![row(1)]);
            }
            let value = || {
                Row::new(vec![Column {
                    name: "__rustorm_parent_0".into(),
                    oid: 23,
                    value: Some("1".into()),
                }])
            };
            Ok(vec![value(), value()])
        });
        assert!(matches!(
            result,
            Err(crate::postgres::Error::UnexpectedResult)
        ));
    }
    #[test]
    fn loading_is_batched_and_preserves_parent_order() {
        let mut calls = 0;
        let loaded = include_many(Select::new("p", &["id"]), descriptor(), |q| {
            calls += 1;
            if calls == 1 {
                Ok((0..1001).rev().map(row).collect())
            } else {
                assert!(q.params.len() <= 500);
                Ok(vec![])
            }
        })
        .unwrap();
        assert_eq!(calls, 4); // root plus three batches
        assert_eq!(loaded.len(), 1001);
        assert_eq!(loaded.first().unwrap().parent.0, 1000);
        assert_eq!(loaded.last().unwrap().parent.0, 0);
        assert!(loaded.iter().all(|entry| entry.children.is_empty()));
    }
    #[test]
    fn internal_alias_remains_short_when_many_columns_collide() {
        const NAMES: &[&str] = &[
            "__rustorm_parent_key",
            "__rustorm_parent_key_",
            "__rustorm_parent_key__",
            "__rustorm_parent_key___",
            "__rustorm_parent_key____",
            "__rustorm_parent_key_____",
            "__rustorm_parent_key______",
            "__rustorm_parent_key_______",
            "__rustorm_parent_key________",
            "__rustorm_parent_key_________",
            "__rustorm_parent_key__________",
            "__rustorm_parent_key___________",
            "__rustorm_parent_key____________",
            "__rustorm_parent_key_____________",
            "__rustorm_parent_key______________",
            "__rustorm_parent_key_______________",
            "__rustorm_parent_key________________",
            "__rustorm_parent_key_________________",
            "__rustorm_parent_key__________________",
            "__rustorm_parent_key___________________",
            "__rustorm_parent_key____________________",
            "__rustorm_parent_key_____________________",
            "__rustorm_parent_key______________________",
            "__rustorm_parent_key_______________________",
            "__rustorm_parent_key________________________",
            "__rustorm_parent_key_________________________",
            "__rustorm_parent_key__________________________",
            "__rustorm_parent_key___________________________",
            "__rustorm_parent_key____________________________",
            "__rustorm_parent_key_____________________________",
            "__rustorm_parent_key______________________________",
            "__rustorm_parent_key_______________________________",
            "__rustorm_parent_key________________________________",
            "__rustorm_parent_key_________________________________",
            "__rustorm_parent_key__________________________________",
            "__rustorm_parent_key___________________________________",
            "__rustorm_parent_key____________________________________",
            "__rustorm_parent_key_____________________________________",
            "__rustorm_parent_key______________________________________",
            "__rustorm_parent_key_______________________________________",
            "__rustorm_parent_key________________________________________",
            "__rustorm_parent_key_________________________________________",
            "__rustorm_parent_key__________________________________________",
            "__rustorm_parent_key___________________________________________",
            "__rustorm_parent_key____________________________________________",
            "__rustorm_parent_key_____________________________________________",
            "__rustorm_parent_key______________________________________________",
            "__rustorm_parent_key_______________________________________________",
            "__rustorm_parent_key________________________________________________",
            "__rustorm_parent_key_________________________________________________",
            "__rustorm_parent_key__________________________________________________",
            "__rustorm_parent_key___________________________________________________",
            "__rustorm_parent_key____________________________________________________",
            "__rustorm_parent_key_____________________________________________________",
            "__rustorm_parent_key______________________________________________________",
        ];
        let relation = Many::<Parent, Child, i32>::new("p", "id", "c", "p_id", NAMES);
        let mut calls = 0;
        let result = include_many(Select::new("p", &["id"]), relation, |q| {
            calls += 1;
            if calls == 1 {
                Ok(vec![row(1)])
            } else {
                let alias = q
                    .sql
                    .split(" AS \"")
                    .nth(1)
                    .unwrap()
                    .split('\"')
                    .next()
                    .unwrap();
                assert!(alias.len() <= 63, "alias too long: {alias}");
                assert!(!NAMES.contains(&alias));
                Ok(vec![])
            }
        })
        .unwrap();
        assert_eq!(result.len(), 1);
    }
    #[test]
    fn empty_roots_skip_child_queries_and_invalid_selection_never_executes() {
        let mut calls = 0;
        let loaded = include_many(Select::new("p", &["id"]), descriptor(), |_| {
            calls += 1;
            Ok(vec![])
        })
        .unwrap();
        assert!(loaded.is_empty());
        assert_eq!(calls, 1);
        let mut calls = 0;
        let wrong = Select::new("other", &["id"]);
        assert!(include_many(wrong, descriptor(), |_| {
            calls += 1;
            Ok(vec![])
        })
        .is_err());
        let projected =
            Select::new("p", &["id"]).select(crate::query::Field::<i32>::new("p", "id"));
        assert!(include_many(projected, descriptor(), |_| {
            calls += 1;
            Ok(vec![])
        })
        .is_err());
        assert_eq!(calls, 0);
    }
}

#[cfg(all(test, feature = "postgres"))]
mod one_tests {
    use super::*;
    use crate::{
        query::Select,
        row::{Column, DecodeError, FromRow, Row},
    };
    #[derive(Debug)]
    struct Child(Option<i32>);
    impl FromRow for Child {
        fn from_row(r: &Row) -> Result<Self, DecodeError> {
            Ok(Self(r.get("parent_id")?))
        }
    }
    #[derive(Debug)]
    struct Parent(i32);
    impl FromRow for Parent {
        fn from_row(r: &Row) -> Result<Self, DecodeError> {
            Ok(Self(r.get("id")?))
        }
    }
    fn row(name: &str, n: Option<i32>) -> Column {
        Column {
            name: name.into(),
            oid: 23,
            value: n.map(|n| n.to_string()),
        }
    }
    fn descriptor(nullable: bool) -> One<Child, Parent, i32> {
        One::new("c", "parent_id", "p", "id", nullable, &["id"])
    }
    #[test]
    fn inverse_loading_batches_deduplicates_and_preserves_nulls_and_order() {
        let mut calls = 0;
        let result = include_one(Select::new("c", &["parent_id"]), descriptor(true), |q| {
            calls += 1;
            if calls == 1 {
                Ok(vec![
                    Row::new(vec![row("parent_id", Some(2))]),
                    Row::new(vec![row("parent_id", None)]),
                    Row::new(vec![row("parent_id", Some(2))]),
                    Row::new(vec![row("parent_id", Some(1))]),
                ])
            } else {
                assert_eq!(q.params.len(), 2);
                Ok(vec![
                    Row::new(vec![row("id", Some(1)), row("__rustorm_input_0", Some(1))]),
                    Row::new(vec![row("id", Some(2)), row("__rustorm_input_0", Some(2))]),
                ])
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(
            result
                .iter()
                .map(|x| (x.child.0, x.parent.as_ref().map(|p| p.0)))
                .collect::<Vec<_>>(),
            vec![
                (Some(2), Some(2)),
                (None, None),
                (Some(2), Some(2)),
                (Some(1), Some(1))
            ]
        );
    }
    #[test]
    fn inverse_loading_uses_bounded_batches_and_skips_null_only_roots() {
        let mut calls = 0;
        let loaded = include_one(Select::new("c", &["parent_id"]), descriptor(true), |q| {
            calls += 1;
            if calls == 1 {
                return Ok((0..1001)
                    .map(|n| Row::new(vec![row("parent_id", Some(n))]))
                    .collect());
            }
            assert!(q.params.len() <= 500);
            // Inputs are sequential in this test, as in the root result.
            let start = (calls - 2) * 500;
            Ok((start..start + q.params.len())
                .map(|n| {
                    Row::new(vec![
                        row("id", Some(n as i32)),
                        row("__rustorm_input_0", Some(n as i32)),
                    ])
                })
                .collect())
        })
        .unwrap();
        assert_eq!(calls, 4);
        assert_eq!(loaded.len(), 1001);
        let mut calls = 0;
        let loaded = include_one(Select::new("c", &["parent_id"]), descriptor(true), |_| {
            calls += 1;
            Ok(vec![Row::new(vec![row("parent_id", None)])])
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(loaded[0].parent.is_none());
    }
    #[test]
    fn inverse_loading_fails_closed_on_missing_or_unexpected_parent_and_invalid_select() {
        assert!(include_one(
            Select::new("wrong", &["parent_id"]),
            descriptor(true),
            |_| panic!()
        )
        .is_err());
        assert!(
            include_one(Select::new("c", &["parent_id"]), descriptor(false), |_| Ok(
                vec![Row::new(vec![row("parent_id", None)])]
            ))
            .is_err()
        );
        let mut calls = 0;
        assert!(
            include_one(Select::new("c", &["parent_id"]), descriptor(true), |_| {
                calls += 1;
                if calls == 1 {
                    Ok(vec![Row::new(vec![row("parent_id", Some(1))])])
                } else {
                    Ok(vec![])
                }
            })
            .is_err()
        );
    }
}
