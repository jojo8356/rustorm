//! Parameterized PostgreSQL query builders. Building does not execute SQL.
use std::marker::PhantomData;

#[derive(Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i32),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            _ => f.write_str("[REDACTED]"),
        }
    }
}

/// Conversion restricted by the generated field's Rust type.
pub trait IntoFieldValue<T> {
    fn into_value(self) -> Value;
}
impl IntoFieldValue<String> for String {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}
impl IntoFieldValue<String> for &str {
    fn into_value(self) -> Value {
        Value::String(self.to_owned())
    }
}
macro_rules! values {
    ($($t:ty => $v:ident),* $(,)?) => {$(impl IntoFieldValue<$t> for $t { fn into_value(self) -> Value { Value::$v(self) } })*};
}
values!(i32 => Int, i64 => BigInt, f64 => Float, bool => Bool);

/// A field tied to a generated model. Its table is checked when it is used.
#[derive(Debug)]
pub struct Field<T> {
    table: &'static str,
    name: &'static str,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for Field<T> {}
impl<T> Clone for Field<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Field<T> {
    #[doc(hidden)]
    pub const fn new(table: &'static str, name: &'static str) -> Self {
        Self {
            table,
            name,
            marker: PhantomData,
        }
    }
    pub(crate) fn table_name(self) -> &'static str {
        self.table
    }
    pub(crate) fn column_name(self) -> &'static str {
        self.name
    }
    pub fn equals(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("=", value.into_value()),
        )
    }
    pub fn not_equals(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<>", value.into_value()),
        )
    }
    /// Match any value in a non-empty list. Values are sent as parameters.
    pub fn is_in<V: IntoFieldValue<T>>(self, values: impl IntoIterator<Item = V>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::In(
                values.into_iter().map(IntoFieldValue::into_value).collect(),
                false,
            ),
        )
    }
    /// Exclude values in a non-empty list; nullable columns retain SQL NULL semantics.
    pub fn not_in<V: IntoFieldValue<T>>(self, values: impl IntoIterator<Item = V>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::In(
                values.into_iter().map(IntoFieldValue::into_value).collect(),
                true,
            ),
        )
    }
    pub fn asc(self) -> Order {
        Order {
            table: self.table,
            name: self.name,
            descending: false,
            nulls_first: None,
        }
    }
    pub fn desc(self) -> Order {
        Order {
            table: self.table,
            name: self.name,
            descending: true,
            nulls_first: None,
        }
    }
}
mod numeric_sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
    impl Sealed for f64 {}
}
/// Numeric field types supported by atomic arithmetic updates.
pub trait NumericField: numeric_sealed::Sealed + IntoFieldValue<Self> + Sized {}
impl NumericField for i32 {}
impl NumericField for i64 {}
impl NumericField for f64 {}

/// Scalar types which admit PostgreSQL ordering comparisons.
pub trait OrderedField {}
impl OrderedField for i32 {}
impl OrderedField for i64 {}
impl OrderedField for f64 {}
impl OrderedField for String {}
impl<T: OrderedField> Field<T> {
    pub fn lt(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<", value.into_value()),
        )
    }
    pub fn lte(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<=", value.into_value()),
        )
    }
    pub fn gt(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare(">", value.into_value()),
        )
    }
    pub fn gte(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare(">=", value.into_value()),
        )
    }
}
fn like_literal(input: &str) -> String {
    let mut escaped = String::new();
    for c in input.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}
impl Field<String> {
    /// Literal substring search (not a wildcard pattern), using the database collation.
    pub fn contains(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("%{}%", like_literal(value)))),
        )
    }
    pub fn starts_with(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("{}%", like_literal(value)))),
        )
    }
    pub fn ends_with(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("%{}", like_literal(value)))),
        )
    }
    /// Case-insensitive literal substring search using PostgreSQL ILIKE and its collation.
    pub fn contains_insensitive(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("%{}%", like_literal(value)))),
        )
    }
    pub fn starts_with_insensitive(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("{}%", like_literal(value)))),
        )
    }
    pub fn ends_with_insensitive(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("%{}", like_literal(value)))),
        )
    }
}
impl<T> Field<Option<T>> {
    /// Sort ascending with NULL values before non-NULL values.
    pub fn asc_nulls_first(self) -> Order {
        let mut order = self.asc();
        order.nulls_first = Some(true);
        order
    }
    /// Sort ascending with NULL values after non-NULL values.
    pub fn asc_nulls_last(self) -> Order {
        let mut order = self.asc();
        order.nulls_first = Some(false);
        order
    }
    /// Sort descending with NULL values before non-NULL values.
    pub fn desc_nulls_first(self) -> Order {
        let mut order = self.desc();
        order.nulls_first = Some(true);
        order
    }
    /// Sort descending with NULL values after non-NULL values.
    pub fn desc_nulls_last(self) -> Order {
        let mut order = self.desc();
        order.nulls_first = Some(false);
        order
    }
    pub fn is_null(self) -> Filter {
        Filter::atom(self.table, self.name, FilterOp::IsNull)
    }
    pub fn is_not_null(self) -> Filter {
        Filter::atom(self.table, self.name, FilterOp::IsNotNull)
    }
    pub fn equals_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("=", value.into_value()),
        )
    }
    pub fn not_equals_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<>", value.into_value()),
        )
    }
    pub fn in_some<V: IntoFieldValue<T>>(self, values: impl IntoIterator<Item = V>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::In(
                values.into_iter().map(IntoFieldValue::into_value).collect(),
                false,
            ),
        )
    }
    pub fn not_in_some<V: IntoFieldValue<T>>(self, values: impl IntoIterator<Item = V>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::In(
                values.into_iter().map(IntoFieldValue::into_value).collect(),
                true,
            ),
        )
    }
}
impl<T: OrderedField> Field<Option<T>> {
    pub fn lt_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<", value.into_value()),
        )
    }
    pub fn lte_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare("<=", value.into_value()),
        )
    }
    pub fn gt_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare(">", value.into_value()),
        )
    }
    pub fn gte_some(self, value: impl IntoFieldValue<T>) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Compare(">=", value.into_value()),
        )
    }
}
impl Field<Option<String>> {
    pub fn contains_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("%{}%", like_literal(value)))),
        )
    }
    pub fn starts_with_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("{}%", like_literal(value)))),
        )
    }
    pub fn ends_with_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::Like(Value::String(format!("%{}", like_literal(value)))),
        )
    }
    /// NULL values do not match, as with PostgreSQL ILIKE.
    pub fn contains_insensitive_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("%{}%", like_literal(value)))),
        )
    }
    pub fn starts_with_insensitive_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("{}%", like_literal(value)))),
        )
    }
    pub fn ends_with_insensitive_some(self, value: &str) -> Filter {
        Filter::atom(
            self.table,
            self.name,
            FilterOp::ILike(Value::String(format!("%{}", like_literal(value)))),
        )
    }
}
#[derive(Debug, Clone)]
enum FilterOp {
    Compare(&'static str, Value),
    In(Vec<Value>, bool),
    Like(Value),
    ILike(Value),
    IsNull,
    IsNotNull,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum RelationQuantifier {
    Some,
    None,
    Every,
}
#[derive(Debug, Clone)]
enum FilterExpr {
    Atom {
        table: &'static str,
        name: &'static str,
        op: FilterOp,
    },
    Relation {
        parent: &'static str,
        child: &'static str,
        parent_key: &'static str,
        child_key: &'static str,
        through: Option<(&'static str, &'static str, &'static str)>,
        quantifier: RelationQuantifier,
        filter: Box<Filter>,
    },
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
}
#[derive(Debug, Clone)]
pub struct Filter {
    expr: FilterExpr,
}
impl Filter {
    fn atom(table: &'static str, name: &'static str, op: FilterOp) -> Self {
        Self {
            expr: FilterExpr::Atom { table, name, op },
        }
    }
    pub(crate) fn relation(
        parent: &'static str,
        child: &'static str,
        parent_key: &'static str,
        child_key: &'static str,
        quantifier: RelationQuantifier,
        filter: Filter,
    ) -> Self {
        Self {
            expr: FilterExpr::Relation {
                parent,
                child,
                parent_key,
                child_key,
                through: None,
                quantifier,
                filter: Box::new(filter),
            },
        }
    }
    pub(crate) fn relation_through(
        parent: &'static str,
        child: &'static str,
        parent_key: &'static str,
        child_key: &'static str,
        link: (&'static str, &'static str, &'static str),
        quantifier: RelationQuantifier,
        filter: Filter,
    ) -> Self {
        Self {
            expr: FilterExpr::Relation {
                parent,
                child,
                parent_key,
                child_key,
                through: Some(link),
                quantifier,
                filter: Box::new(filter),
            },
        }
    }
    pub fn and(filters: Vec<Filter>) -> Self {
        Self {
            expr: FilterExpr::And(filters),
        }
    }
    pub fn or(filters: Vec<Filter>) -> Self {
        Self {
            expr: FilterExpr::Or(filters),
        }
    }
    pub fn negate(filter: Filter) -> Self {
        Self {
            expr: FilterExpr::Not(Box::new(filter)),
        }
    }
    // Only conjunctions preserve a unique equality. Never descend into OR or NOT.
    fn has_unique_equality(&self, table: &str, keys: &[&str]) -> bool {
        match &self.expr {
            FilterExpr::Atom {
                table: actual,
                name,
                op: FilterOp::Compare("=", value),
            } => *actual == table && keys.contains(name) && !matches!(value, Value::Null),
            FilterExpr::And(filters) => filters.iter().any(|f| f.has_unique_equality(table, keys)),
            _ => false,
        }
    }
    fn validate(&self, table: &str, depth: usize) -> Result<(), QueryError> {
        if depth > 64 {
            return Err(QueryError::FilterTooDeep);
        }
        match &self.expr {
            FilterExpr::Atom { table: actual, .. } if *actual != table => {
                Err(QueryError::WrongModel)
            }
            FilterExpr::Atom {
                op: FilterOp::In(values, _),
                ..
            } => {
                if values.is_empty() {
                    return Err(QueryError::EmptyValueList);
                }
                if values.len() > MAX_PARAMETERS {
                    return Err(QueryError::TooManyParameters);
                }
                if values.iter().any(|value| matches!(value, Value::Null)) {
                    return Err(QueryError::NullInValueList);
                }
                Ok(())
            }
            FilterExpr::Atom { .. } => Ok(()),
            FilterExpr::Relation {
                parent,
                child,
                filter,
                ..
            } => {
                if *parent != table {
                    return Err(QueryError::WrongModel);
                }
                filter.validate(child, depth + 1)
            }
            FilterExpr::And(v) | FilterExpr::Or(v) => {
                if v.is_empty() {
                    return Err(QueryError::EmptyFilterGroup);
                }
                for child in v {
                    child.validate(table, depth + 1)?;
                }
                Ok(())
            }
            FilterExpr::Not(child) => child.validate(table, depth + 1),
        }
    }
    fn render(self, params: &mut Vec<Value>) -> String {
        self.render_qualified(params, None, 0)
    }
    fn render_qualified(
        self,
        params: &mut Vec<Value>,
        qualifier: Option<&str>,
        depth: usize,
    ) -> String {
        let qualified = |name: &str| match qualifier {
            Some(q) => format!("{}.{}", quote(q), quote(name)),
            None => quote(name),
        };
        match self.expr {
            FilterExpr::Atom { name, op, .. } => match op {
                FilterOp::Compare(operator, value) => {
                    params.push(value);
                    format!("{} {operator} ${}", qualified(name), params.len())
                }
                FilterOp::In(values, negated) => {
                    let placeholders = values
                        .into_iter()
                        .map(|value| {
                            params.push(value);
                            format!("${}", params.len())
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{} {}IN ({placeholders})",
                        qualified(name),
                        if negated { "NOT " } else { "" }
                    )
                }
                FilterOp::Like(value) => {
                    params.push(value);
                    format!("{} LIKE ${} ESCAPE E'\\\\'", qualified(name), params.len())
                }
                FilterOp::ILike(value) => {
                    params.push(value);
                    format!("{} ILIKE ${} ESCAPE E'\\\\'", qualified(name), params.len())
                }
                FilterOp::IsNull => format!("{} IS NULL", qualified(name)),
                FilterOp::IsNotNull => format!("{} IS NOT NULL", qualified(name)),
            },
            FilterExpr::Relation {
                parent,
                child,
                parent_key,
                child_key,
                through,
                quantifier,
                filter,
            } => {
                let outer = qualifier.unwrap_or(parent);
                let mut alias = format!("__rustorm_rel_{depth}");
                while alias == outer {
                    alias.push('_');
                }
                let predicate = filter.render_qualified(params, Some(&alias), depth + 1);
                let condition = match quantifier {
                    RelationQuantifier::Every => format!("({predicate}) IS NOT TRUE"),
                    _ => predicate,
                };
                let negate = if matches!(quantifier, RelationQuantifier::Some) {
                    ""
                } else {
                    "NOT "
                };
                if let Some((table, local, remote)) = through {
                    let mut link_alias = format!("__rustorm_link_{depth}");
                    while link_alias == outer || link_alias == alias {
                        link_alias.push('_');
                    }
                    format!("{negate}EXISTS (SELECT 1 FROM {} AS {} JOIN {} AS {} ON {}.{} = {}.{} WHERE {}.{} = {}.{} AND ({}))", quote(child), quote(&alias), quote(table), quote(&link_alias), quote(&alias), quote(child_key), quote(&link_alias), quote(remote), quote(&link_alias), quote(local), quote(outer), quote(parent_key), condition)
                } else {
                    format!(
                        "{negate}EXISTS (SELECT 1 FROM {} AS {} WHERE {}.{} = {}.{} AND ({}))",
                        quote(child),
                        quote(&alias),
                        quote(&alias),
                        quote(child_key),
                        quote(outer),
                        quote(parent_key),
                        condition
                    )
                }
            }
            FilterExpr::And(v) => format!(
                "({})",
                v.into_iter()
                    .map(|f| f.render_qualified(params, qualifier, depth))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
            FilterExpr::Or(v) => format!(
                "({})",
                v.into_iter()
                    .map(|f| f.render_qualified(params, qualifier, depth))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
            FilterExpr::Not(f) => format!("NOT ({})", f.render_qualified(params, qualifier, depth)),
        }
    }
}
#[derive(Debug, Clone)]
pub struct Order {
    table: &'static str,
    name: &'static str,
    descending: bool,
    nulls_first: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledQuery {
    pub sql: String,
    pub params: Vec<Value>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    WrongModel,
    EmptySelection,
    PaginationOutOfRange,
    MissingRequiredField(&'static str),
    DuplicateField,
    EmptyMutation,
    UnfilteredMutation,
    EmptyFilterGroup,
    FilterTooDeep,
    EmptyValueList,
    NullInValueList,
    TooManyParameters,
    DistinctCountNeedsField,
    DivisionByZero,
    NonFiniteArithmeticOperand,
    DistinctOrderNotSelected,
    NestedReturningUnsupported,
    EmptyNestedWrite,
    TooManyNestedWrites,
    DuplicateNestedTarget,
    NestedRelationKeyMutation,
    RequiredRelationViolation,
    NonUniqueNestedSelector,
}
impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::WrongModel => "field belongs to a different model",
            Self::EmptySelection => "selection has no fields",
            Self::PaginationOutOfRange => "pagination exceeds PostgreSQL bigint range",
            Self::MissingRequiredField(_) => "required field is missing",
            Self::DuplicateField => "field is specified more than once",
            Self::EmptyMutation => "mutation has no values",
            Self::UnfilteredMutation => "refusing a mutation without a filter",
            Self::EmptyFilterGroup => "filter groups cannot be empty",
            Self::FilterTooDeep => "filter nesting exceeds the maximum depth",
            Self::EmptyValueList => "IN and NOT IN require a non-empty list",
            Self::NullInValueList => {
                "IN and NOT IN do not accept NULL; use an explicit null filter"
            }
            Self::TooManyParameters => "query exceeds the PostgreSQL parameter limit",
            Self::DistinctCountNeedsField => "distinct count requires a field",
            Self::DivisionByZero => "cannot divide by zero",
            Self::NonFiniteArithmeticOperand => "arithmetic operands must be finite",
            Self::DistinctOrderNotSelected => "DISTINCT ordering fields must be selected",
            Self::NestedReturningUnsupported => "nested inserts manage RETURNING internally",
            Self::EmptyNestedWrite => "nested creation requires at least one child",
            Self::TooManyNestedWrites => "nested creation exceeds the child limit",
            Self::DuplicateNestedTarget => "nested connect cannot contain duplicate keys",
            Self::NestedRelationKeyMutation => "nested update cannot change the owning foreign key",
            Self::RequiredRelationViolation => "a required foreign key cannot be disconnected",
            Self::NonUniqueNestedSelector => {
                "nested selector must constrain a unique scalar field with a non-null equality"
            }
        })
    }
}
impl std::error::Error for QueryError {}
/// Maximum number of parameters in a PostgreSQL extended-protocol query.
pub const MAX_PARAMETERS: usize = 65_535;
fn check_parameter_count(params: &[Value]) -> Result<(), QueryError> {
    if params.len() > MAX_PARAMETERS {
        Err(QueryError::TooManyParameters)
    } else {
        Ok(())
    }
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Constructed by generated model modules. Filters from another model are rejected.
#[derive(Debug, Clone)]
pub struct Select {
    table: &'static str,
    columns: &'static [&'static str],
    selection: Vec<(&'static str, &'static str)>,
    distinct: bool,
    filters: Vec<Filter>,
    orders: Vec<Order>,
    skip: Option<u64>,
    take: Option<u64>,
}
impl Select {
    #[doc(hidden)]
    pub fn new(table: &'static str, columns: &'static [&'static str]) -> Self {
        Self {
            table,
            columns,
            selection: vec![],
            distinct: false,
            filters: vec![],
            orders: vec![],
            skip: None,
            take: None,
        }
    }
    #[cfg(feature = "postgres")]
    pub(crate) fn supports_relation(&self, table: &str, key: &str) -> bool {
        self.table == table && self.selection.is_empty() && self.columns.contains(&key)
    }
    /// Select only the specified fields; subsequent calls add fields in order.
    /// Partial rows do not decode into a full generated Record.
    pub fn select<T>(mut self, field: Field<T>) -> Self {
        self.selection.push((field.table, field.name));
        self
    }
    /// Remove duplicate rows based on the entire selected projection.
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }
    pub fn where_(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn order_by(mut self, order: Order) -> Self {
        self.orders.push(order);
        self
    }
    pub fn skip(mut self, count: u64) -> Self {
        self.skip = Some(count);
        self
    }
    pub fn take(mut self, count: u64) -> Self {
        self.take = Some(count);
        self
    }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        let columns: Vec<&str> = if self.selection.is_empty() {
            self.columns.to_vec()
        } else {
            let mut seen = std::collections::HashSet::new();
            for &(table, name) in &self.selection {
                if table != self.table {
                    return Err(QueryError::WrongModel);
                }
                if !seen.insert(name) {
                    return Err(QueryError::DuplicateField);
                }
            }
            self.selection.iter().map(|(_, name)| *name).collect()
        };
        if columns.is_empty() {
            return Err(QueryError::EmptySelection);
        }
        for filter in &self.filters {
            filter.validate(self.table, 0)?;
        }
        if self.orders.iter().any(|o| o.table != self.table) {
            return Err(QueryError::WrongModel);
        }
        if self.distinct && self.orders.iter().any(|o| !columns.contains(&o.name)) {
            return Err(QueryError::DistinctOrderNotSelected);
        }
        if self.skip.is_some_and(|v| v > i64::MAX as u64)
            || self.take.is_some_and(|v| v > i64::MAX as u64)
        {
            return Err(QueryError::PaginationOutOfRange);
        }
        let mut sql = format!(
            "SELECT {}{} FROM {}",
            if self.distinct { "DISTINCT " } else { "" },
            columns
                .iter()
                .map(|s| quote(s))
                .collect::<Vec<_>>()
                .join(", "),
            quote(self.table)
        );
        let mut params = Vec::new();
        if !self.filters.is_empty() {
            sql.push_str(" WHERE ");
            let parts: Vec<_> = self
                .filters
                .into_iter()
                .map(|f| f.render(&mut params))
                .collect();
            sql.push_str(&parts.join(" AND "));
        }
        if !self.orders.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .orders
                    .iter()
                    .map(|o| {
                        format!(
                            "{} {}{}",
                            quote(o.name),
                            if o.descending { "DESC" } else { "ASC" },
                            match o.nulls_first {
                                Some(true) => " NULLS FIRST",
                                Some(false) => " NULLS LAST",
                                None => "",
                            },
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if let Some(n) = self.take {
            params.push(Value::BigInt(n as i64));
            sql.push_str(&format!(" LIMIT ${}", params.len()));
        }
        if let Some(n) = self.skip {
            params.push(Value::BigInt(n as i64));
            sql.push_str(&format!(" OFFSET ${}", params.len()));
        }
        check_parameter_count(&params)?;
        Ok(CompiledQuery { sql, params })
    }
}

/// Count all rows or those matching the supplied filters. The result is a PostgreSQL bigint.
#[derive(Debug, Clone)]
pub struct Count {
    table: &'static str,
    filters: Vec<Filter>,
    field: Option<(&'static str, &'static str)>,
    distinct: bool,
}
impl Count {
    #[doc(hidden)]
    pub fn new(table: &'static str) -> Self {
        Self {
            table,
            filters: Vec::new(),
            field: None,
            distinct: false,
        }
    }
    /// Count non-NULL values of a field instead of rows.
    pub fn field<T>(mut self, field: Field<T>) -> Self {
        self.field = Some((field.table, field.name));
        self
    }
    /// Count distinct non-NULL values; requires a field.
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }
    pub fn where_(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        for filter in &self.filters {
            filter.validate(self.table, 0)?;
        }
        let expression = match self.field {
            Some((table, _)) if table != self.table => return Err(QueryError::WrongModel),
            Some((_, name)) if self.distinct => format!("DISTINCT {}", quote(name)),
            Some((_, name)) => quote(name),
            None if self.distinct => return Err(QueryError::DistinctCountNeedsField),
            None => "*".into(),
        };
        let mut sql = format!(
            "SELECT COUNT({expression}) AS \"count\" FROM {}",
            quote(self.table)
        );
        let mut params = Vec::new();
        if !self.filters.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(
                &self
                    .filters
                    .into_iter()
                    .map(|filter| filter.render(&mut params))
                    .collect::<Vec<_>>()
                    .join(" AND "),
            );
        }
        check_parameter_count(&params)?;
        Ok(CompiledQuery { sql, params })
    }
}
/// Row returned by a count query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountResult {
    pub count: i64,
}
impl crate::row::FromRow for CountResult {
    fn from_row(row: &crate::row::Row) -> Result<Self, crate::row::DecodeError> {
        Ok(Self {
            count: row.get("count")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentOp {
    Set,
    Increment,
    Decrement,
    Multiply,
    Divide,
}
#[derive(Debug, Clone)]
struct Assignment {
    table: &'static str,
    name: &'static str,
    value: Value,
    op: AssignmentOp,
}
fn assignment<T>(field: Field<T>, value: impl IntoFieldValue<T>) -> Assignment {
    Assignment {
        table: field.table,
        name: field.name,
        value: value.into_value(),
        op: AssignmentOp::Set,
    }
}
fn optional_assignment<T>(field: Field<Option<T>>, value: impl IntoFieldValue<T>) -> Assignment {
    Assignment {
        table: field.table,
        name: field.name,
        value: value.into_value(),
        op: AssignmentOp::Set,
    }
}
fn null_assignment<T>(field: Field<Option<T>>) -> Assignment {
    Assignment {
        table: field.table,
        name: field.name,
        value: Value::Null,
        op: AssignmentOp::Set,
    }
}
fn arithmetic_assignment<T: NumericField>(
    table: &'static str,
    name: &'static str,
    value: T,
    op: AssignmentOp,
) -> Assignment {
    Assignment {
        table,
        name,
        value: value.into_value(),
        op,
    }
}
fn check_assignments(table: &str, assignments: &[Assignment]) -> Result<(), QueryError> {
    let mut names = std::collections::HashSet::new();
    for a in assignments {
        if a.table != table {
            return Err(QueryError::WrongModel);
        }
        if !names.insert(a.name) {
            return Err(QueryError::DuplicateField);
        }
        if a.op != AssignmentOp::Set {
            if let Value::Float(value) = a.value {
                if !value.is_finite() {
                    return Err(QueryError::NonFiniteArithmeticOperand);
                }
            }
            if a.op == AssignmentOp::Divide
                && match a.value {
                    Value::Int(v) => v == 0,
                    Value::BigInt(v) => v == 0,
                    Value::Float(v) => v == 0.0,
                    _ => false,
                }
            {
                return Err(QueryError::DivisionByZero);
            }
        }
    }
    Ok(())
}
fn append_filters(
    sql: &mut String,
    params: &mut Vec<Value>,
    table: &str,
    filters: Vec<Filter>,
) -> Result<(), QueryError> {
    if filters.is_empty() {
        return Err(QueryError::UnfilteredMutation);
    }
    for filter in &filters {
        filter.validate(table, 0)?;
    }
    sql.push_str(" WHERE ");
    let expressions: Vec<_> = filters
        .into_iter()
        .map(|filter| filter.render(params))
        .collect();
    sql.push_str(&expressions.join(" AND "));
    Ok(())
}

fn append_returning(
    sql: &mut String,
    columns: Option<&'static [&'static str]>,
) -> Result<(), QueryError> {
    if let Some(columns) = columns {
        if columns.is_empty() {
            return Err(QueryError::EmptySelection);
        }
        sql.push_str(" RETURNING ");
        sql.push_str(
            &columns
                .iter()
                .map(|name| quote(name))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    Ok(())
}

/// Insert with runtime validation for required fields. Values are always parameters.
#[derive(Debug, Clone)]
pub struct Insert {
    table: &'static str,
    required: &'static [&'static str],
    assignments: Vec<Assignment>,
    returning: Option<&'static [&'static str]>,
}
impl Insert {
    #[doc(hidden)]
    pub fn new(table: &'static str, required: &'static [&'static str]) -> Self {
        Self {
            table,
            required,
            assignments: vec![],
            returning: None,
        }
    }
    pub fn set<T>(mut self, field: Field<T>, value: impl IntoFieldValue<T>) -> Self {
        self.assignments.push(assignment(field, value));
        self
    }
    pub fn set_some<T>(mut self, field: Field<Option<T>>, value: impl IntoFieldValue<T>) -> Self {
        self.assignments.push(optional_assignment(field, value));
        self
    }
    pub fn set_null<T>(mut self, field: Field<Option<T>>) -> Self {
        self.assignments.push(null_assignment(field));
        self
    }
    pub fn returning(mut self, columns: &'static [&'static str]) -> Self {
        self.returning = Some(columns);
        self
    }
    pub(crate) fn nested_parent(
        self,
        table: &'static str,
        key: &'static str,
    ) -> Result<CompiledQuery, QueryError> {
        if self.table != table {
            return Err(QueryError::WrongModel);
        }
        if self.returning.is_some() {
            return Err(QueryError::NestedReturningUnsupported);
        }
        let mut query = self.compile()?;
        query.sql.push_str(&format!(" RETURNING {}", quote(key)));
        Ok(query)
    }
    pub(crate) fn nested_child(
        mut self,
        table: &'static str,
        key: &'static str,
    ) -> Result<CompiledQuery, QueryError> {
        if self.table != table {
            return Err(QueryError::WrongModel);
        }
        if self.returning.is_some() {
            return Err(QueryError::NestedReturningUnsupported);
        }
        // The placeholder is replaced by the freshly returned parent key at execution.
        self.assignments.push(Assignment {
            table,
            name: key,
            value: Value::Null,
            op: AssignmentOp::Set,
        });
        self.compile()
    }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        check_assignments(self.table, &self.assignments)?;
        for &field in self.required {
            if !self.assignments.iter().any(|a| a.name == field) {
                return Err(QueryError::MissingRequiredField(field));
            }
        }
        if self.assignments.is_empty() {
            let mut sql = format!("INSERT INTO {} DEFAULT VALUES", quote(self.table));
            append_returning(&mut sql, self.returning)?;
            return Ok(CompiledQuery {
                sql,
                params: vec![],
            });
        }
        let columns = self
            .assignments
            .iter()
            .map(|a| quote(a.name))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=self.assignments.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!(
            "INSERT INTO {} ({columns}) VALUES ({placeholders})",
            quote(self.table)
        );
        append_returning(&mut sql, self.returning)?;
        let params: Vec<_> = self.assignments.into_iter().map(|a| a.value).collect();
        check_parameter_count(&params)?;
        Ok(CompiledQuery { sql, params })
    }
}
/// An UPDATE requires at least one assignment and one filter.
#[derive(Debug, Clone)]
pub struct Update {
    table: &'static str,
    assignments: Vec<Assignment>,
    filters: Vec<Filter>,
    returning: Option<&'static [&'static str]>,
}
impl Update {
    pub(crate) fn nested_disconnect(
        table: &'static str,
        key: &'static str,
        parent: Value,
        filter: Filter,
        unique_fields: &'static [&'static str],
        composite_key: &'static [&'static str],
    ) -> Result<CompiledQuery, QueryError> {
        filter.validate(table, 0)?;
        let scalar_unique = filter.has_unique_equality(table, unique_fields);
        let composite_unique = composite_key.len() >= 2
            && composite_key
                .iter()
                .all(|field| filter.has_unique_equality(table, std::slice::from_ref(field)));
        if !scalar_unique && !composite_unique {
            return Err(QueryError::NonUniqueNestedSelector);
        }
        if matches!(parent, Value::Null) {
            return Err(QueryError::NullInValueList);
        }
        Self {
            table,
            assignments: vec![Assignment {
                table,
                name: key,
                value: Value::Null,
                op: AssignmentOp::Set,
            }],
            filters: vec![
                filter,
                Filter::atom(table, key, FilterOp::Compare("=", parent)),
            ],
            returning: None,
        }
        .compile()
    }
    pub(crate) fn nested_target(
        self,
        table: &'static str,
        key: &'static str,
        target: Value,
    ) -> Result<CompiledQuery, QueryError> {
        let filter = Filter::atom(table, key, FilterOp::Compare("=", target.clone()));
        self.nested_scoped(table, key, target, filter)
    }
    pub(crate) fn nested_scoped(
        mut self,
        table: &'static str,
        key: &'static str,
        parent: Value,
        filter: Filter,
    ) -> Result<CompiledQuery, QueryError> {
        if self.table != table {
            return Err(QueryError::WrongModel);
        }
        if self.returning.is_some() {
            return Err(QueryError::NestedReturningUnsupported);
        }
        if self
            .assignments
            .iter()
            .any(|a| a.name == key && a.table == table)
        {
            return Err(QueryError::NestedRelationKeyMutation);
        }
        if matches!(parent, Value::Null) {
            return Err(QueryError::NullInValueList);
        }
        self.filters.push(filter);
        self.filters
            .push(Filter::atom(table, key, FilterOp::Compare("=", parent)));
        self.compile()
    }
    #[doc(hidden)]
    pub fn new(table: &'static str) -> Self {
        Self {
            table,
            assignments: vec![],
            filters: vec![],
            returning: None,
        }
    }
    pub fn set<T>(mut self, field: Field<T>, value: impl IntoFieldValue<T>) -> Self {
        self.assignments.push(assignment(field, value));
        self
    }
    pub fn set_some<T>(mut self, field: Field<Option<T>>, value: impl IntoFieldValue<T>) -> Self {
        self.assignments.push(optional_assignment(field, value));
        self
    }
    pub fn set_null<T>(mut self, field: Field<Option<T>>) -> Self {
        self.assignments.push(null_assignment(field));
        self
    }
    /// Apply a parameterized arithmetic operation atomically in PostgreSQL.
    pub fn increment<T: NumericField>(mut self, field: Field<T>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Increment,
        ));
        self
    }
    /// As with SQL arithmetic, a NULL field remains NULL.
    pub fn increment_some<T: NumericField>(mut self, field: Field<Option<T>>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Increment,
        ));
        self
    }
    /// Apply a parameterized arithmetic operation atomically in PostgreSQL.
    pub fn decrement<T: NumericField>(mut self, field: Field<T>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Decrement,
        ));
        self
    }
    /// As with SQL arithmetic, a NULL field remains NULL.
    pub fn decrement_some<T: NumericField>(mut self, field: Field<Option<T>>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Decrement,
        ));
        self
    }
    /// Apply a parameterized arithmetic operation atomically in PostgreSQL.
    pub fn multiply<T: NumericField>(mut self, field: Field<T>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Multiply,
        ));
        self
    }
    /// As with SQL arithmetic, a NULL field remains NULL.
    pub fn multiply_some<T: NumericField>(mut self, field: Field<Option<T>>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Multiply,
        ));
        self
    }
    /// Apply a parameterized arithmetic operation atomically in PostgreSQL.
    pub fn divide<T: NumericField>(mut self, field: Field<T>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Divide,
        ));
        self
    }
    /// As with SQL arithmetic, a NULL field remains NULL.
    pub fn divide_some<T: NumericField>(mut self, field: Field<Option<T>>, value: T) -> Self {
        self.assignments.push(arithmetic_assignment(
            field.table,
            field.name,
            value,
            AssignmentOp::Divide,
        ));
        self
    }
    pub fn where_(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn returning(mut self, columns: &'static [&'static str]) -> Self {
        self.returning = Some(columns);
        self
    }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        check_assignments(self.table, &self.assignments)?;
        if self.assignments.is_empty() {
            return Err(QueryError::EmptyMutation);
        }
        let mut params = Vec::new();
        let parts: Vec<_> = self
            .assignments
            .into_iter()
            .map(|a| {
                params.push(a.value);
                let name = quote(a.name);
                match a.op {
                    AssignmentOp::Set => format!("{name} = ${}", params.len()),
                    op => {
                        let operator = match op {
                            AssignmentOp::Increment => "+",
                            AssignmentOp::Decrement => "-",
                            AssignmentOp::Multiply => "*",
                            AssignmentOp::Divide => "/",
                            AssignmentOp::Set => unreachable!(),
                        };
                        format!("{name} = {name} {operator} ${}", params.len())
                    }
                }
            })
            .collect();
        let mut sql = format!("UPDATE {} SET {}", quote(self.table), parts.join(", "));
        append_filters(&mut sql, &mut params, self.table, self.filters)?;
        append_returning(&mut sql, self.returning)?;
        check_parameter_count(&params)?;
        Ok(CompiledQuery { sql, params })
    }
}
/// A DELETE requires at least one filter; a full-table delete is not supported.
#[derive(Debug, Clone)]
pub struct Delete {
    table: &'static str,
    filters: Vec<Filter>,
    returning: Option<&'static [&'static str]>,
}
impl Delete {
    pub(crate) fn nested_target(
        self,
        table: &'static str,
        key: &'static str,
        target: Value,
    ) -> Result<CompiledQuery, QueryError> {
        let filter = Filter::atom(table, key, FilterOp::Compare("=", target.clone()));
        self.nested_scoped(table, key, target, filter)
    }
    pub(crate) fn nested_scoped(
        mut self,
        table: &'static str,
        key: &'static str,
        parent: Value,
        filter: Filter,
    ) -> Result<CompiledQuery, QueryError> {
        if self.table != table {
            return Err(QueryError::WrongModel);
        }
        if self.returning.is_some() {
            return Err(QueryError::NestedReturningUnsupported);
        }
        if matches!(parent, Value::Null) {
            return Err(QueryError::NullInValueList);
        }
        self.filters.push(filter);
        self.filters
            .push(Filter::atom(table, key, FilterOp::Compare("=", parent)));
        self.compile()
    }
    #[doc(hidden)]
    pub fn new(table: &'static str) -> Self {
        Self {
            table,
            filters: vec![],
            returning: None,
        }
    }
    pub fn where_(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn returning(mut self, columns: &'static [&'static str]) -> Self {
        self.returning = Some(columns);
        self
    }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        let mut sql = format!("DELETE FROM {}", quote(self.table));
        let mut params = Vec::new();
        append_filters(&mut sql, &mut params, self.table, self.filters)?;
        append_returning(&mut sql, self.returning)?;
        check_parameter_count(&params)?;
        Ok(CompiledQuery { sql, params })
    }
}
