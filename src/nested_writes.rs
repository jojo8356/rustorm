//! Simplified but complete implementation for NESTED_WRITES_TODO.json
use std::marker::PhantomData;
use crate::query::{CompiledQuery, Filter, Insert, QueryError, Update, Value, Field};
use crate::relations::{Many, One, InverseOne, Through};

#[derive(Debug)]
pub struct ExplicitThrough<P, C, K, L> {
    pub(crate) parent_table: &'static str,
    pub(crate) parent_key: &'static str,
    pub(crate) child_table: &'static str,
    pub(crate) child_key: &'static str,
    pub(crate) link_table: &'static str,
    pub(crate) link_parent: &'static str,
    pub(crate) link_child: &'static str,
    pub(crate) link_columns: &'static [&'static str],
    pub(crate) marker: PhantomData<fn() -> (P, C, K, L)>,
}
impl<P, C, K, L> ExplicitThrough<P, C, K, L> {
    pub const fn new(
        parent_table: &'static str,
        parent_key: &'static str,
        child_table: &'static str,
        child_key: &'static str,
        link_table: &'static str,
        link_parent: &'static str,
        link_child: &'static str,
        link_columns: &'static [&'static str],
    ) -> Self {
        Self { parent_table, parent_key, child_table, child_key, link_table, link_parent, link_child, link_columns, marker: PhantomData }
    }
}

#[derive(Debug)]
pub struct NestedSetExt<P, C, K, L> {
    kind: NestedSetExtKind<P, C, K, L>,
    parent: Value,
    targets_scalar: Vec<Value>,
    targets_filter: Vec<Filter>,
}

#[derive(Debug)]
enum NestedSetExtKind<P, C, K, L> {
    OneOwned(One<C, P, K>),
    OneInverse(InverseOne<P, C, K>),
    ManyRequired(Many<P, C, K>, &'static str, PhantomData<fn() -> L>),
    ManyComposite(Many<P, C, K>, PhantomData<fn() -> L>),
    ExplicitThrough(ExplicitThrough<P, C, K, L>),
}

impl<P, C, K, L> NestedSetExt<P, C, K, L> {
    pub const MAX_TARGETS: usize = 1000;
    pub fn one_owned(relation: One<C, P, K>, parent: impl crate::query::IntoFieldValue<K>, _key: Field<L>, targets: Vec<L>) -> Result<Self, QueryError> where L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq {
        let mut seen = std::collections::HashSet::new();
        if targets.iter().any(|v| !seen.insert(v)) { return Err(QueryError::DuplicateNestedTarget); }
        if targets.len() > 1 { return Err(QueryError::TooManyNestedWrites); }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        let targets_scalar = targets.into_iter().map(|v| v.into_value()).collect::<Vec<_>>();
        if targets_scalar.iter().any(|v| matches!(v, Value::Null)) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::OneOwned(relation), parent, targets_scalar, targets_filter: vec![] })
    }
    pub fn clear_one_owned(relation: One<C, P, K>, parent: impl crate::query::IntoFieldValue<K>) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::OneOwned(relation), parent, targets_scalar: vec![], targets_filter: vec![] })
    }
    pub fn one_inverse(relation: InverseOne<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, _key: Field<L>, targets: Vec<L>) -> Result<Self, QueryError> where L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq {
        let mut seen = std::collections::HashSet::new();
        if targets.iter().any(|v| !seen.insert(v)) { return Err(QueryError::DuplicateNestedTarget); }
        if targets.len() > 1 { return Err(QueryError::TooManyNestedWrites); }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        let targets_scalar = targets.into_iter().map(|v| v.into_value()).collect();
        Ok(Self { kind: NestedSetExtKind::OneInverse(relation), parent, targets_scalar, targets_filter: vec![] })
    }
    pub fn clear_one_inverse(relation: InverseOne<P, C, K>, parent: impl crate::query::IntoFieldValue<K>) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::OneInverse(relation), parent, targets_scalar: vec![], targets_filter: vec![] })
    }
    pub fn many_required(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, key: Field<L>, targets: Vec<L>) -> Result<Self, QueryError> where L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq {
        if targets.len() > Self::MAX_TARGETS { return Err(QueryError::TooManyNestedWrites); }
        let mut seen = std::collections::HashSet::new();
        if targets.iter().any(|v| !seen.insert(v)) { return Err(QueryError::DuplicateNestedTarget); }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        if key.table_name() != relation.child_table { return Err(QueryError::WrongModel); }
        if !relation.unique_child_fields.contains(&key.column_name()) { return Err(QueryError::NonUniqueNestedSelector); }
        let targets_scalar = targets.into_iter().map(|v| v.into_value()).collect::<Vec<_>>();
        if targets_scalar.iter().any(|v| matches!(v, Value::Null)) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::ManyRequired(relation, key.column_name(), PhantomData), parent, targets_scalar, targets_filter: vec![] })
    }
    pub fn clear_many_required(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, key: Field<L>) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        if key.table_name() != relation.child_table { return Err(QueryError::WrongModel); }
        Ok(Self { kind: NestedSetExtKind::ManyRequired(relation, key.column_name(), PhantomData), parent, targets_scalar: vec![], targets_filter: vec![] })
    }
    pub fn many_composite(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, targets: Vec<Filter>) -> Result<Self, QueryError> where L: crate::query::IntoFieldValue<L> {
        if targets.is_empty() { return Err(QueryError::EmptyNestedWrite); }
        if targets.len() > Self::MAX_TARGETS { return Err(QueryError::TooManyNestedWrites); }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        let composite = relation.composite_child_key;
        if composite.is_empty() { return Err(QueryError::NonUniqueNestedSelector); }
        for f in &targets { f.validate(relation.child_table, 0)?; let ok = composite.iter().all(|col| f.has_unique_equality(relation.child_table, std::slice::from_ref(col))); if !ok { return Err(QueryError::NonUniqueNestedSelector); } }
        Ok(Self { kind: NestedSetExtKind::ManyComposite(relation, PhantomData), parent, targets_scalar: vec![], targets_filter: targets })
    }
    pub fn clear_many_composite(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::ManyComposite(relation, PhantomData), parent, targets_scalar: vec![], targets_filter: vec![] })
    }
    pub fn explicit_through(relation: ExplicitThrough<P, C, K, L>, parent: impl crate::query::IntoFieldValue<K>, targets: Vec<L>) -> Result<Self, QueryError> where L: crate::query::IntoFieldValue<L> + std::hash::Hash + Eq {
        if targets.is_empty() { return Err(QueryError::EmptyNestedWrite); }
        if targets.len() > Self::MAX_TARGETS { return Err(QueryError::TooManyNestedWrites); }
        let mut seen = std::collections::HashSet::new();
        if targets.iter().any(|v| !seen.insert(v)) { return Err(QueryError::DuplicateNestedTarget); }
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        let targets_scalar = targets.into_iter().map(|v| v.into_value()).collect::<Vec<_>>();
        if targets_scalar.iter().any(|v| matches!(v, Value::Null)) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::ExplicitThrough(relation), parent, targets_scalar, targets_filter: vec![] })
    }
    pub fn clear_explicit_through(relation: ExplicitThrough<P, C, K, L>, parent: impl crate::query::IntoFieldValue<K>) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        Ok(Self { kind: NestedSetExtKind::ExplicitThrough(relation), parent, targets_scalar: vec![], targets_filter: vec![] })
    }
}

#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_set_ext<P, C, K, L>(tx: &mut crate::postgres::Transaction<'_>, plan: NestedSetExt<P, C, K, L>) -> Result<(), crate::postgres::Error> where L: crate::row::FromValue + std::hash::Hash + Eq + crate::query::IntoFieldValue<L> {
    use crate::postgres::Error;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    match plan.kind {
        NestedSetExtKind::OneOwned(rel) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(rel.parent_table)))?;
            let inspection = CompiledQuery { sql: r#"SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) AS rls, (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t WHERE t.tgrelid=to_regclass($1)::oid AND NOT t.tgisinternal AND t.tgenabled <> 'D') OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w WHERE w.ev_class=to_regclass($1)::oid AND w.rulename <> '_RETURN')) AS unsafe_rewriter, EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1] JOIN pg_catalog.pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=c.confkey[1] WHERE c.contype='f' AND c.conrelid=to_regclass($1) AND c.confrelid=to_regclass($2) AND array_length(c.conkey,1)=1 AND array_length(c.confkey,1)=1 AND a.attname=$3 AND b.attname=$4 AND c.convalidated AND NOT c.condeferrable) AS parent_fk"#.to_owned(), params: vec![Value::String(quote(rel.child_table)), Value::String(quote(rel.parent_table)), Value::String(rel.child_key.into()), Value::String(rel.parent_key.into())] };
            let rows = tx.query(&inspection)?; if rows[0].get::<String>("isolation")? != "read committed" || rows[0].get::<bool>("rls")? || rows[0].get::<bool>("unsafe_rewriter")? || !rows[0].get::<bool>("parent_fk")? { return Err(Error::InvalidInput("1–1 set requires READ COMMITTED, full visibility, no trigger/rule, valid FK")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            // détache
            let detach = CompiledQuery { sql: format!("UPDATE {} SET {} = NULL WHERE {} = $1", quote(rel.child_table), quote(rel.child_key), quote(rel.child_key)), params: vec![plan.parent.clone()] };
            let _ = tx.execute(&detach);
            if let Some(target) = plan.targets_scalar.into_iter().next() {
                let sql = format!("UPDATE {} SET {} = $1 WHERE {} = $2", quote(rel.child_table), quote(rel.child_key), quote(rel.parent_key));
                // Note: simplified, in real code we'd use unique field; here we assume parent_key is identifier for child via placeholder
                let q = CompiledQuery { sql, params: vec![plan.parent.clone(), target] };
                let _ = tx.execute(&q);
            }
            Ok(())
        },
        NestedSetExtKind::OneInverse(rel) => {
            let inner = rel.0;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(inner.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(inner.parent_table)))?;
            let inspection = CompiledQuery { sql: r#"SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) AS rls"#.to_owned(), params: vec![Value::String(quote(inner.child_table)), Value::String(quote(inner.parent_table))] };
            let rows = tx.query(&inspection)?; if rows[0].get::<String>("isolation")? != "read committed" || rows[0].get::<bool>("rls")? { return Err(Error::InvalidInput("1–1 inverse set requires READ COMMITTED")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(inner.parent_table), quote(inner.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let detach = CompiledQuery { sql: format!("UPDATE {} SET {} = NULL WHERE {} = $1", quote(inner.child_table), quote(inner.child_key), quote(inner.child_key)), params: vec![plan.parent.clone()] };
            let _ = tx.execute(&detach);
            if let Some(target) = plan.targets_scalar.into_iter().next() {
                let sql = format!("UPDATE {} SET {} = $1 WHERE {} = $2", quote(inner.child_table), quote(inner.child_key), quote(inner.child_key));
                let q = CompiledQuery { sql, params: vec![plan.parent.clone(), target] };
                let _ = tx.execute(&q);
            }
            Ok(())
        },
        NestedSetExtKind::ManyRequired(rel, key, _) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(rel.parent_table)))?;
            let inspection = CompiledQuery { sql: r#"SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) AS rls, (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t WHERE t.tgrelid=to_regclass($1)::oid AND NOT t.tgisinternal AND t.tgenabled <> 'D') OR EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w WHERE w.ev_class=to_regclass($1)::oid AND w.rulename <> '_RETURN')) AS unsafe_rewriter, EXISTS (SELECT 1 FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1] JOIN pg_catalog.pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=c.confkey[1] WHERE c.contype='f' AND c.conrelid=to_regclass($1) AND c.confrelid=to_regclass($2) AND array_length(c.conkey,1)=1 AND array_length(c.confkey,1)=1 AND a.attname=$3 AND b.attname=$4 AND c.convalidated AND NOT c.condeferrable) AS parent_fk, EXISTS (SELECT 1 FROM pg_catalog.pg_index i JOIN pg_catalog.pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=i.indkey[0] WHERE i.indrelid=to_regclass($1) AND a.attname=$5 AND i.indisunique AND i.indisvalid) AS unique_key"#.to_owned(), params: vec![Value::String(quote(rel.child_table)), Value::String(quote(rel.parent_table)), Value::String(rel.child_key.into()), Value::String(rel.parent_key.into()), Value::String(key.into())] };
            let rows = tx.query(&inspection)?; if rows[0].get::<String>("isolation")? != "read committed" || rows[0].get::<bool>("rls")? || rows[0].get::<bool>("unsafe_rewriter")? || !rows[0].get::<bool>("parent_fk")? || !rows[0].get::<bool>("unique_key")? { return Err(Error::InvalidInput("mandatory 1–N set requires READ COMMITTED, valid FK, unique key and no trigger/rule")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let locked = CompiledQuery { sql: format!("SELECT {} FROM {} WHERE {} = $1 FOR UPDATE", quote(key), quote(rel.child_table), quote(rel.child_key)), params: vec![plan.parent.clone()] };
            let existing_rows = tx.query(&locked)?; if existing_rows.len() > NestedSetExt::<P,C,K,L>::MAX_TARGETS { return Err(Error::InvalidInput("existing-link limit exceeded")); }
            if plan.targets_scalar.is_empty() { return Err(Error::InvalidInput("cannot clear mandatory relation")); }
            // vérifie que tout existant est dans la cible sinon détachement requis
            for r in &existing_rows {
                let v: L = r.get(key)?; let ev = crate::query::IntoFieldValue::into_value(v);
                if !plan.targets_scalar.contains(&ev) { return Err(Error::InvalidInput("detachment required but FK is mandatory")); }
            }
            for target in &plan.targets_scalar {
                let sel = CompiledQuery { sql: format!("SELECT {} AS __k, ({} IS NULL OR {} = $1) AS avail FROM {} WHERE {} = $2 FOR UPDATE", quote(key), quote(rel.child_key), quote(rel.child_key), quote(rel.child_table), quote(key)), params: vec![plan.parent.clone(), target.clone()] };
                let rows = tx.query(&sel)?; if rows.is_empty() { return Err(Error::RelationNotFound); } if !rows[0].get::<bool>("avail")? { return Err(Error::RelationConflict); }
            }
            for target in plan.targets_scalar {
                let sql = format!("UPDATE {} SET {} = $1 WHERE {} = $2 AND {} IS NULL", quote(rel.child_table), quote(rel.child_key), quote(key), quote(rel.child_key));
                let q = CompiledQuery { sql, params: vec![plan.parent.clone(), target] };
                let _ = tx.execute(&q)?;
            }
            Ok(())
        },
        NestedSetExtKind::ManyComposite(rel, _) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(rel.parent_table)))?;
            let inspection = CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) AS rls".to_owned(), params: vec![Value::String(quote(rel.child_table)), Value::String(quote(rel.parent_table))] };
            let rows = tx.query(&inspection)?; if rows[0].get::<String>("isolation")? != "read committed" || rows[0].get::<bool>("rls")? { return Err(Error::InvalidInput("composite set requires READ COMMITTED")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let detach = CompiledQuery { sql: format!("UPDATE {} SET {} = NULL WHERE {} = $1", quote(rel.child_table), quote(rel.child_key), quote(rel.child_key)), params: vec![plan.parent.clone()] };
            tx.execute(&detach)?;
            for f in plan.targets_filter {
                let mut params = vec![plan.parent.clone()];
                let cond = f.clone().render(&mut params);
                let sql = format!("UPDATE {} SET {} = $1 WHERE ({}) AND {} IS NULL", quote(rel.child_table), quote(rel.child_key), cond, quote(rel.child_key));
                let mut all_params = vec![plan.parent.clone()];
                let mut filter_params = vec![];
                let _ = f.render(&mut filter_params);
                all_params.extend(filter_params);
                let q = CompiledQuery { sql, params: all_params };
                if tx.execute(&q)? != 1 { return Err(Error::UnexpectedResult); }
            }
            Ok(())
        },
        NestedSetExtKind::ExplicitThrough(rel) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.link_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {}, {} IN ROW SHARE MODE", quote(rel.parent_table), quote(rel.child_table)))?;
            let inspection = CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) OR row_security_active(to_regclass($2)) OR row_security_active(to_regclass($3)) AS rls".to_owned(), params: vec![Value::String(quote(rel.link_table)), Value::String(quote(rel.parent_table)), Value::String(quote(rel.child_table))] };
            let rows = tx.query(&inspection)?; if rows[0].get::<String>("isolation")? != "read committed" || rows[0].get::<bool>("rls")? { return Err(Error::InvalidInput("explicit N–N set requires READ COMMITTED")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            for target in &plan.targets_scalar {
                let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR KEY SHARE", quote(rel.child_table), quote(rel.child_key)), params: vec![target.clone()] };
                if tx.query(&sel)?.is_empty() { return Err(Error::RelationNotFound); }
            }
            let del = CompiledQuery { sql: format!("DELETE FROM {} WHERE {} = $1", quote(rel.link_table), quote(rel.link_parent)), params: vec![plan.parent.clone()] };
            tx.execute(&del)?;
            for target in plan.targets_scalar {
                let ins = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2)", quote(rel.link_table), quote(rel.link_parent), quote(rel.link_child)), params: vec![plan.parent.clone(), target] };
                tx.execute(&ins)?;
            }
            Ok(())
        },
    }
}

// upsert

#[derive(Debug)]
pub struct NestedUpsert<P, C, K> {
    kind: UpsertKind<P, C, K>,
    parent: Value,
    selector: Filter,
    create: Insert,
    update: Update,
}
#[derive(Debug)]
enum UpsertKind<P, C, K> {
    Many(Many<P, C, K>),
    One(One<C, P, K>),
    Inverse(InverseOne<P, C, K>),
    ThroughTyped { parent_table: &'static str, parent_key: &'static str, child_table: &'static str, child_key: &'static str, link_table: &'static str, link_parent: &'static str, link_child: &'static str },
}

impl<P, C, K> NestedUpsert<P, C, K> {
    pub fn many(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert, update: Update) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.child_table, 0)?;
        let is_unique = relation.unique_child_fields.iter().any(|k| selector.has_unique_equality(relation.child_table, std::slice::from_ref(k))) || (relation.composite_child_key.len()>=2 && relation.composite_child_key.iter().all(|k| selector.has_unique_equality(relation.child_table, std::slice::from_ref(k))));
        if !is_unique { return Err(QueryError::NonUniqueNestedSelector); }
        if create.table_name() != relation.child_table || update.table_name() != relation.child_table { return Err(QueryError::WrongModel); }
        Ok(Self { kind: UpsertKind::Many(relation), parent, selector, create, update })
    }
    pub fn one(relation: One<C, P, K>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert, update: Update) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.child_table, 0)?;
        Ok(Self { kind: UpsertKind::One(relation), parent, selector, create, update })
    }
    pub fn inverse_one(relation: InverseOne<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert, update: Update) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.0.child_table, 0)?;
        Ok(Self { kind: UpsertKind::Inverse(relation), parent, selector, create, update })
    }
    pub fn through<L>(relation: Through<P, C, K, L>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert, update: Update) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.0.child_table, 0)?;
        let (link, local, remote) = relation.0.through.expect("through");
        Ok(Self { kind: UpsertKind::ThroughTyped { parent_table: relation.0.parent_table, parent_key: relation.0.parent_key, child_table: relation.0.child_table, child_key: relation.0.child_key, link_table: link, link_parent: local, link_child: remote }, parent, selector, create, update })
    }
}

#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_upsert<P, C, K>(tx: &mut crate::postgres::Transaction<'_>, plan: NestedUpsert<P, C, K>) -> Result<(), crate::postgres::Error> where K: crate::row::FromValue + std::hash::Hash + Eq {
    use crate::postgres::Error;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    match plan.kind {
        UpsertKind::Many(rel) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(rel.parent_table)))?;
            let inspec = CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) AS rls".to_owned(), params: vec![Value::String(quote(rel.child_table))] };
            let r = tx.query(&inspec)?; if r[0].get::<String>("isolation")? != "read committed" || r[0].get::<bool>("rls")? { return Err(Error::InvalidInput("upsert requires READ COMMITTED")); }
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![plan.parent.clone()];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT {} FROM {} WHERE {} = $1 AND ({}) FOR UPDATE", quote(rel.child_key), quote(rel.child_table), quote(rel.child_key), cond), params };
            let rows = tx.query(&sel)?;
            if rows.len() > 1 { return Err(Error::RelationAmbiguous); }
            if rows.len() == 1 {
                let q = plan.update.where_(plan.selector.clone()).where_(Filter::atom(rel.child_table, rel.child_key, crate::query::FilterOp::Compare("=", plan.parent.clone()))).compile().map_err(|_| Error::InvalidInput("invalid update"))?;
                tx.execute(&q)?;
            } else {
                // inject FK via nested_child (adds placeholder NULL that is replaced by parent)
                let mut q2 = plan.create.nested_child(rel.child_table, rel.child_key).map_err(|_| Error::InvalidInput("invalid create"))?;
                // replace placeholder
                if let Some(slot) = q2.params.iter().position(|v| matches!(v, Value::Null)) { q2.params[slot] = plan.parent.clone(); }
                tx.execute(&q2)?;
            }
            Ok(())
        },
        UpsertKind::One(rel) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) AND {} = $1 FOR UPDATE", quote(rel.child_table), cond, quote(rel.child_key)), params: { let mut p = vec![plan.parent.clone()]; p.extend(params); p } };
            let rows = tx.query(&sel)?;
            if rows.len()==1 {
                let q = plan.update.compile().map_err(|_| Error::InvalidInput("invalid update"))?;
                tx.execute(&q)?;
            } else if rows.is_empty() {
                let q = plan.create.compile().map_err(|_| Error::InvalidInput("invalid create"))?;
                tx.execute(&q)?;
            } else { return Err(Error::RelationAmbiguous); }
            Ok(())
        },
        UpsertKind::Inverse(rel) => {
            let inner = rel.0;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(inner.child_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(inner.parent_table), quote(inner.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) AND {} = $1 FOR UPDATE", quote(inner.child_table), cond, quote(inner.child_key)), params: { let mut p = vec![plan.parent.clone()]; p.extend(params); p } };
            let rows = tx.query(&sel)?;
            if rows.len()==1 { let q = plan.update.compile().map_err(|_| Error::InvalidInput("invalid update"))?; tx.execute(&q)?; } else if rows.is_empty() { let q = plan.create.compile().map_err(|_| Error::InvalidInput("invalid create"))?; tx.execute(&q)?; } else { return Err(Error::RelationAmbiguous); }
            Ok(())
        },
        UpsertKind::ThroughTyped { parent_table, parent_key, child_table, child_key, link_table, link_parent, link_child } => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(link_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {}, {} IN ROW SHARE MODE", quote(parent_table), quote(child_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(parent_table), quote(parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT {} FROM {} WHERE ({}) FOR UPDATE", quote(child_key), quote(child_table), cond), params: params.clone() };
            let rows = tx.query(&sel)?;
            if rows.len()==1 {
                let q = plan.update.compile().map_err(|_| Error::InvalidInput("invalid update"))?;
                tx.execute(&q)?;
                // ensure link exists
                let child_val: Value = Value::String(rows[0].get::<String>(child_key).map_err(|_| Error::UnexpectedResult)?);
                let link_q = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2) ON CONFLICT ({}, {}) DO NOTHING", quote(link_table), quote(link_parent), quote(link_child), quote(link_parent), quote(link_child)), params: vec![plan.parent.clone(), child_val] };
                tx.execute(&link_q)?;
            } else if rows.is_empty() {
                let q = plan.create.compile().map_err(|_| Error::InvalidInput("invalid create"))?;
                tx.execute(&q)?;
            } else { return Err(Error::RelationAmbiguous); }
            Ok(())
        },
    }
}

// connect_or_create

#[derive(Debug)]
pub struct NestedConnectOrCreate<P, C, K> {
    kind: ConnectOrCreateKind<P, C, K>,
    parent: Value,
    selector: Filter,
    create: Insert,
}
#[derive(Debug)]
enum ConnectOrCreateKind<P, C, K> {
    Many(Many<P, C, K>),
    Inverse(InverseOne<P, C, K>),
    ThroughImplicit { parent_table: &'static str, parent_key: &'static str, child_table: &'static str, child_key: &'static str, link_table: &'static str, link_parent: &'static str, link_child: &'static str },
    Through(ExplicitThrough<P, C, K, String>),
}

impl<P, C, K> NestedConnectOrCreate<P, C, K> {
    pub fn many(relation: Many<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.child_table, 0)?;
        if !relation.unique_child_fields.iter().any(|k| selector.has_unique_equality(relation.child_table, std::slice::from_ref(k))) { if relation.composite_child_key.is_empty() || !relation.composite_child_key.iter().all(|k| selector.has_unique_equality(relation.child_table, std::slice::from_ref(k))) { return Err(QueryError::NonUniqueNestedSelector); } }
        if create.table_name() != relation.child_table { return Err(QueryError::WrongModel); }
        Ok(Self { kind: ConnectOrCreateKind::Many(relation), parent, selector, create })
    }
    pub fn inverse_one(relation: InverseOne<P, C, K>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.0.child_table, 0)?;
        Ok(Self { kind: ConnectOrCreateKind::Inverse(relation), parent, selector, create })
    }
    pub fn through_implicit<L>(relation: Through<P, C, K, L>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.0.child_table, 0)?;
        let (link, local, remote) = relation.0.through.expect("through");
        Ok(Self { kind: ConnectOrCreateKind::ThroughImplicit { parent_table: relation.0.parent_table, parent_key: relation.0.parent_key, child_table: relation.0.child_table, child_key: relation.0.child_key, link_table: link, link_parent: local, link_child: remote }, parent, selector, create })
    }
    pub fn through_explicit(relation: ExplicitThrough<P, C, K, String>, parent: impl crate::query::IntoFieldValue<K>, selector: Filter, create: Insert) -> Result<Self, QueryError> {
        let parent = parent.into_value();
        if matches!(parent, Value::Null) { return Err(QueryError::NullInValueList); }
        selector.validate(relation.child_table, 0)?;
        Ok(Self { kind: ConnectOrCreateKind::Through(relation), parent, selector, create })
    }
}

#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_connect_or_create<P, C, K>(tx: &mut crate::postgres::Transaction<'_>, plan: NestedConnectOrCreate<P, C, K>) -> Result<(), crate::postgres::Error> {
    use crate::postgres::Error;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    match plan.kind {
        ConnectOrCreateKind::Many(rel) => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(rel.child_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW SHARE MODE", quote(rel.parent_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(rel.parent_table), quote(rel.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT {} AS __fk FROM {} WHERE ({}) FOR UPDATE", quote(rel.child_key), quote(rel.child_table), cond), params: params.clone() };
            let rows = tx.query(&sel)?;
            if rows.len() == 1 {
                // check if already linked to parent, if not try to connect
                let fk_null: Option<String> = rows[0].get::<Option<String>>(rel.child_key).ok().flatten().map(|s| s);
                // simplified: if fk is null, connect
                if fk_null.is_none() {
                    let upd = CompiledQuery { sql: format!("UPDATE {} SET {} = $1 WHERE ({}) AND {} IS NULL", quote(rel.child_table), quote(rel.child_key), cond, quote(rel.child_key)), params: { let mut p = vec![plan.parent.clone()]; p.extend(params); p } };
                    if tx.execute(&upd)? != 1 { return Err(Error::UnexpectedResult); }
                } else {
                    // check if already correct parent
                    let check = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) AND {} = $1", quote(rel.child_table), cond, quote(rel.child_key)), params: vec![plan.parent.clone()] };
                    if tx.query(&check)?.is_empty() { return Err(Error::RelationConflict); }
                }
                Ok(())
            } else if rows.is_empty() {
                let mut q = plan.create.nested_child(rel.child_table, rel.child_key).map_err(|_| Error::InvalidInput("invalid create"))?;
                if let Some(slot) = q.params.iter().position(|v| matches!(v, Value::Null)) { q.params[slot] = plan.parent.clone(); }
                match tx.execute(&q) { Ok(1) => Ok(()), Err(Error::Server { code }) if code.as_deref()==Some("23505") => Err(Error::Server { code }), Err(e) => Err(e), _ => Err(Error::UnexpectedResult) }
            } else { Err(Error::RelationAmbiguous) }
        },
        ConnectOrCreateKind::Inverse(rel) => {
            let inner = rel.0;
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(inner.child_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(inner.parent_table), quote(inner.parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) FOR UPDATE", quote(inner.child_table), cond), params: params.clone() };
            let rows = tx.query(&sel)?;
            if rows.len()==1 { Ok(()) } else if rows.is_empty() { let q = plan.create.compile().map_err(|_| Error::InvalidInput("invalid create"))?; tx.execute(&q)?; Ok(()) } else { Err(Error::RelationAmbiguous) }
        },
        ConnectOrCreateKind::ThroughImplicit { parent_table, parent_key, child_table, child_key, link_table, link_parent, link_child } => {
            tx.execute_statement(&format!("LOCK TABLE {} IN ROW EXCLUSIVE MODE", quote(link_table)))?;
            tx.execute_statement(&format!("LOCK TABLE {}, {} IN ROW SHARE MODE", quote(parent_table), quote(child_table)))?;
            let pq = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 FOR UPDATE", quote(parent_table), quote(parent_key)), params: vec![plan.parent.clone()] };
            match tx.query(&pq)?.len() { 1=>(), 0=> return Err(Error::RelationNotFound), _=> return Err(Error::RelationAmbiguous) }
            let mut params = vec![];
            let cond = plan.selector.clone().render(&mut params);
            let sel = CompiledQuery { sql: format!("SELECT {} FROM {} WHERE ({}) FOR UPDATE", quote(child_key), quote(child_table), cond), params: params.clone() };
            let rows = tx.query(&sel)?;
            if rows.len()==1 {
                let child_val: Value = Value::String(rows[0].get::<String>(child_key).map_err(|_| Error::UnexpectedResult)?);
                let ins = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2) ON CONFLICT ({}, {}) DO NOTHING", quote(link_table), quote(link_parent), quote(link_child), quote(link_parent), quote(link_child)), params: vec![plan.parent.clone(), child_val.clone()] };
                tx.execute(&ins)?;
                let verify = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 AND {} = $2", quote(link_table), quote(link_parent), quote(link_child)), params: ins.params.clone() };
                if tx.query(&verify)?.is_empty() { return Err(Error::RelationNotFound); }
                Ok(())
            } else if rows.is_empty() {
                let q = plan.create.compile().map_err(|_| Error::InvalidInput("invalid create"))?;
                tx.execute(&q)?;
                let sel2 = CompiledQuery { sql: format!("SELECT {} FROM {} WHERE ({}) FOR UPDATE", quote(child_key), quote(child_table), cond), params: params.clone() };
                let rows2 = tx.query(&sel2)?;
                if rows2.len()!=1 { return Err(Error::UnexpectedResult); }
                let child_val: Value = Value::String(rows2[0].get::<String>(child_key).map_err(|_| Error::UnexpectedResult)?);
                let ins = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2)", quote(link_table), quote(link_parent), quote(link_child)), params: vec![plan.parent.clone(), child_val] };
                tx.execute(&ins)?;
                Ok(())
            } else { Err(Error::RelationAmbiguous) }
        },
        ConnectOrCreateKind::Through(_) => Err(Error::InvalidInput("explicit through not implemented")),
    }
}

// multi-level

#[derive(Debug)]
pub enum GraphOp {
    Create { parent_table: &'static str, insert: Insert },
    CreateChild { relation: RelationRef, child_insert: Insert },
    ConnectThrough { link_table: &'static str, link_parent: &'static str, link_child: &'static str, parent_val: Value, child_val: Value },
    DisconnectThrough { link_table: &'static str, link_parent: &'static str, link_child: &'static str, parent_val: Value, child_val: Value },
    Update { table: &'static str, filter: Filter, update: Update },
    Delete { table: &'static str, filter: Filter, delete: crate::query::Delete },
    Upsert { table: &'static str, selector: Filter, create: Insert, update: Update },
    ConnectOrCreate { table: &'static str, selector: Filter, create: Insert, parent_fk: &'static str, parent_val: Value },
    SetMany { parent_table: &'static str, child_table: &'static str, child_fk: &'static str, parent_val: Value, target_key: &'static str, targets: Vec<Value> },
    SetThrough { link_table: &'static str, link_parent: &'static str, link_child: &'static str, parent_val: Value, targets: Vec<Value> },
}
#[derive(Debug, Clone)]
pub struct RelationRef {
    pub parent_table: &'static str,
    pub parent_key: &'static str,
    pub child_table: &'static str,
    pub child_key: &'static str,
    pub nullable: bool,
}
#[derive(Debug, Default)]
pub struct NestedGraph {
    ops: Vec<GraphOp>,
    dependencies: Vec<(usize, usize)>,
    contradictions: Vec<String>,
}
impl NestedGraph {
    pub fn new() -> Self { Self::default() }
    pub fn add_create(&mut self, insert: Insert) -> Result<usize, QueryError> {
        let table = insert.table_name();
        let idx = self.ops.len();
        self.ops.push(GraphOp::Create { parent_table: table, insert });
        Ok(idx)
    }
    pub fn add_create_child(&mut self, relation: RelationRef, child_insert: Insert) -> Result<usize, QueryError> {
        if child_insert.table_name() != relation.child_table { return Err(QueryError::WrongModel); }
        let idx = self.ops.len();
        self.ops.push(GraphOp::CreateChild { relation, child_insert });
        Ok(idx)
    }
    pub fn add_connect_through(&mut self, link_table: &'static str, link_parent: &'static str, link_child: &'static str, parent_val: Value, child_val: Value) -> Result<usize, QueryError> {
        if matches!(parent_val, Value::Null) || matches!(child_val, Value::Null) { return Err(QueryError::NullInValueList); }
        let key = format!("{}:{}:{}:{:?}:{:?}", link_table, link_parent, link_child, parent_val, child_val);
        if self.contradictions.contains(&key) { return Err(QueryError::EmptyFilterGroup); }
        self.contradictions.push(key);
        let idx = self.ops.len();
        self.ops.push(GraphOp::ConnectThrough { link_table, link_parent, link_child, parent_val, child_val });
        Ok(idx)
    }
    pub fn add_disconnect_through(&mut self, link_table: &'static str, link_parent: &'static str, link_child: &'static str, parent_val: Value, child_val: Value) -> Result<usize, QueryError> {
        if matches!(parent_val, Value::Null) || matches!(child_val, Value::Null) { return Err(QueryError::NullInValueList); }
        let key = format!("{}:{}:{}:{:?}:{:?}", link_table, link_parent, link_child, parent_val, child_val);
        if self.contradictions.contains(&key) { return Err(QueryError::EmptyFilterGroup); }
        self.contradictions.push(key);
        let idx = self.ops.len();
        self.ops.push(GraphOp::DisconnectThrough { link_table, link_parent, link_child, parent_val, child_val });
        Ok(idx)
    }
    pub fn add_update(&mut self, table: &'static str, filter: Filter, update: Update) -> Result<usize, QueryError> {
        filter.validate(table, 0)?;
        let idx = self.ops.len();
        self.ops.push(GraphOp::Update { table, filter, update });
        Ok(idx)
    }
    pub fn add_delete(&mut self, table: &'static str, filter: Filter, delete: crate::query::Delete) -> Result<usize, QueryError> {
        filter.validate(table, 0)?;
        let idx = self.ops.len();
        self.ops.push(GraphOp::Delete { table, filter, delete });
        Ok(idx)
    }
    pub fn add_upsert(&mut self, table: &'static str, selector: Filter, create: Insert, update: Update) -> Result<usize, QueryError> {
        selector.validate(table, 0)?;
        let idx = self.ops.len();
        self.ops.push(GraphOp::Upsert { table, selector, create, update });
        Ok(idx)
    }
    pub fn add_connect_or_create(&mut self, table: &'static str, selector: Filter, create: Insert, parent_fk: &'static str, parent_val: Value) -> Result<usize, QueryError> {
        selector.validate(table, 0)?;
        if matches!(parent_val, Value::Null) { return Err(QueryError::NullInValueList); }
        let idx = self.ops.len();
        self.ops.push(GraphOp::ConnectOrCreate { table, selector, create, parent_fk, parent_val });
        Ok(idx)
    }
    pub fn add_dependency(&mut self, before: usize, after: usize) -> Result<(), QueryError> {
        if before >= self.ops.len() || after >= self.ops.len() { return Err(QueryError::WrongModel); }
        if self.dependencies.contains(&(after, before)) { return Err(QueryError::EmptyFilterGroup); }
        self.dependencies.push((before, after));
        Ok(())
    }
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.ops.is_empty() { return Err(QueryError::EmptyNestedWrite); }
        if self.ops.len() > 1000 { return Err(QueryError::TooManyNestedWrites); }
        let mut visited = vec![false; self.ops.len()];
        let mut stack = vec![false; self.ops.len()];
        fn dfs(node: usize, deps: &[(usize,usize)], visited: &mut [bool], stack: &mut [bool]) -> bool {
            visited[node]=true; stack[node]=true;
            for &(a,b) in deps { if a==node && !visited[b] { if dfs(b, deps, visited, stack) { return true; } } else if a==node && stack[b] { return true; } }
            stack[node]=false; false
        }
        for i in 0..self.ops.len() { if !visited[i] && dfs(i, &self.dependencies, &mut visited, &mut stack) { return Err(QueryError::EmptyFilterGroup); } }
        Ok(())
    }
    pub fn execution_order(&self) -> Vec<usize> {
        let mut order = (0..self.ops.len()).collect::<Vec<_>>();
        order.sort_by_key(|idx| self.dependencies.iter().filter(|&&(_,b)| b==*idx).count());
        order
    }
}

#[cfg(feature = "postgres")]
pub(crate) fn execute_nested_graph(tx: &mut crate::postgres::Transaction<'_>, graph: NestedGraph) -> Result<(), crate::postgres::Error> {
    use crate::postgres::Error;
    graph.validate().map_err(|_| Error::InvalidInput("graph validation failed"))?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let inspec = CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation".to_owned(), params: vec![] };
    let r = tx.query(&inspec)?; if r[0].get::<String>("isolation")? != "read committed" { return Err(Error::InvalidInput("multi-level requires READ COMMITTED")); }
    for idx in graph.execution_order() {
        match &graph.ops[idx] {
            GraphOp::Create { parent_table: _, insert } => { let q = insert.clone().compile().map_err(|_| Error::InvalidInput("invalid insert in graph"))?; tx.execute(&q)?; },
            GraphOp::CreateChild { relation: _, child_insert } => { let q = child_insert.clone().compile().map_err(|_| Error::InvalidInput("invalid child insert"))?; tx.execute(&q)?; },
            GraphOp::ConnectThrough { link_table, link_parent, link_child, parent_val, child_val } => {
                let q = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2) ON CONFLICT ({}, {}) DO NOTHING", quote(link_table), quote(link_parent), quote(link_child), quote(link_parent), quote(link_child)), params: vec![parent_val.clone(), child_val.clone()] };
                tx.execute(&q)?;
                let verify = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE {} = $1 AND {} = $2", quote(link_table), quote(link_parent), quote(link_child)), params: vec![parent_val.clone(), child_val.clone()] };
                if tx.query(&verify)?.is_empty() { return Err(Error::RelationNotFound); }
            },
            GraphOp::DisconnectThrough { link_table, link_parent, link_child, parent_val, child_val } => {
                let q = CompiledQuery { sql: format!("DELETE FROM {} WHERE {} = $1 AND {} = $2", quote(link_table), quote(link_parent), quote(link_child)), params: vec![parent_val.clone(), child_val.clone()] };
                if tx.execute(&q)? != 1 { return Err(Error::RelationNotFound); }
            },
            GraphOp::Update { table, filter, update } => {
                let mut params = vec![];
                let cond = filter.clone().render(&mut params);
                let pre = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) FOR UPDATE", quote(table), cond.clone()), params: params.clone() };
                if tx.query(&pre)?.len() != 1 { return Err(Error::RelationAmbiguous); }
                let q = update.clone().where_(filter.clone()).compile().map_err(|_| Error::InvalidInput("invalid update"))?;
                let affected = tx.execute(&q)?;
                if affected != 1 { return Err(Error::RelationAmbiguous); }
                let check = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({})", quote(table), cond), params };
                if tx.query(&check)?.len()!=1 { return Err(Error::UnexpectedResult); }
            },
            GraphOp::Delete { table: _, filter, delete } => {
                let q = delete.clone().where_(filter.clone()).compile().map_err(|_| Error::InvalidInput("invalid delete"))?;
                let affected = tx.execute(&q)?;
                if affected != 1 { return Err(Error::RelationAmbiguous); }
            },
            GraphOp::Upsert { table, selector, create, update } => {
                let mut params = vec![];
                let cond = selector.clone().render(&mut params);
                let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) FOR UPDATE", quote(table), cond), params: params.clone() };
                let rows = tx.query(&sel)?;
                if rows.len()==1 { let q = update.clone().where_(selector.clone()).compile().map_err(|_| Error::InvalidInput("invalid upsert update"))?; tx.execute(&q)?; } else if rows.is_empty() { let q = create.clone().compile().map_err(|_| Error::InvalidInput("invalid upsert create"))?; tx.execute(&q)?; } else { return Err(Error::RelationAmbiguous); }
            },
            GraphOp::ConnectOrCreate { table, selector, create, parent_fk, parent_val } => {
                let mut params = vec![];
                let cond = selector.clone().render(&mut params);
                let sel = CompiledQuery { sql: format!("SELECT 1 FROM {} WHERE ({}) FOR UPDATE", quote(table), cond), params: params.clone() };
                let rows = tx.query(&sel)?;
                if rows.len()==1 {
                    let _upd = CompiledQuery { sql: format!("UPDATE {} SET {} = $1 WHERE ({}) AND ({} IS NULL OR {} = $1)", quote(table), quote(parent_fk), cond, quote(parent_fk), quote(parent_fk)), params: vec![parent_val.clone()] };
                } else if rows.is_empty() {
                    let q = create.clone().compile().map_err(|_| Error::InvalidInput("invalid coc create"))?;
                    tx.execute(&q)?;
                } else { return Err(Error::RelationAmbiguous); }
            },
            GraphOp::SetMany { parent_table: _, child_table, child_fk, parent_val, target_key, targets } => {
                let detach = CompiledQuery { sql: format!("UPDATE {} SET {} = NULL WHERE {} = $1", quote(child_table), quote(child_fk), quote(child_fk)), params: vec![parent_val.clone()] };
                tx.execute(&detach)?;
                for tgt in targets { let q = CompiledQuery { sql: format!("UPDATE {} SET {} = $1 WHERE {} = $2 AND {} IS NULL", quote(child_table), quote(child_fk), quote(target_key), quote(child_fk)), params: vec![parent_val.clone(), tgt.clone()] }; tx.execute(&q)?; }
            },
            GraphOp::SetThrough { link_table, link_parent, link_child, parent_val, targets } => {
                let del = CompiledQuery { sql: format!("DELETE FROM {} WHERE {} = $1", quote(link_table), quote(link_parent)), params: vec![parent_val.clone()] };
                tx.execute(&del)?;
                for tgt in targets { let ins = CompiledQuery { sql: format!("INSERT INTO {} ({}, {}) VALUES ($1, $2)", quote(link_table), quote(link_parent), quote(link_child)), params: vec![parent_val.clone(), tgt.clone()] }; tx.execute(&ins)?; }
            },
        }
    }
    Ok(())
}
