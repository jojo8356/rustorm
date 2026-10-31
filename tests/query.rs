use rustorm::query::{Field, QueryError, Select, Value};
const ID: Field<i32> = Field::new("users", "id");
const EMAIL: Field<String> = Field::new("users", "email");
const NAME: Field<Option<String>> = Field::new("users", "name");
const OTHER: Field<i32> = Field::new("posts", "id");
fn users() -> Select {
    Select::new("users", &["id", "email", "name"])
}
#[test]
fn parameters_prevent_injection_and_preserve_order() {
    let hostile = "' OR 1=1 -- %_";
    let q = users()
        .where_(EMAIL.equals(hostile))
        .where_(ID.equals(7))
        .order_by(ID.desc())
        .take(10)
        .skip(2)
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE \"email\" = $1 AND \"id\" = $2 ORDER BY \"id\" DESC LIMIT $3 OFFSET $4");
    assert_eq!(
        q.params,
        vec![
            Value::String(hostile.into()),
            Value::Int(7),
            Value::BigInt(10),
            Value::BigInt(2)
        ]
    );
    assert!(!q.sql.contains(hostile));
}
#[test]
fn nullable_filters_use_sql_null_semantics() {
    let q = users()
        .where_(NAME.is_null())
        .where_(NAME.is_not_null())
        .where_(NAME.equals_some("Alice"))
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE \"name\" IS NULL AND \"name\" IS NOT NULL AND \"name\" = $1");
    assert_eq!(q.params, vec![Value::String("Alice".into())]);
}
#[test]
fn rejects_cross_model_fields_and_invalid_pagination() {
    assert_eq!(
        users().where_(OTHER.equals(1)).compile().unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        users().order_by(OTHER.asc()).compile().unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        users().take(u64::MAX).compile().unwrap_err(),
        QueryError::PaginationOutOfRange
    );
    assert_eq!(
        Select::new("x", &[]).compile().unwrap_err(),
        QueryError::EmptySelection
    );
}
#[test]
fn quotes_identifiers() {
    let q = Select::new("weird\"table", &["a\"b"]).compile().unwrap();
    assert_eq!(q.sql, "SELECT \"a\"\"b\" FROM \"weird\"\"table\"");
}
#[test]
fn mutations_are_parameterized_and_guarded() {
    use rustorm::query::{Delete, Insert, Update};
    let insert = Insert::new("users", &["email"])
        .set(EMAIL, "'unsafe")
        .set_null(NAME)
        .compile()
        .unwrap();
    assert_eq!(
        insert.sql,
        "INSERT INTO \"users\" (\"email\", \"name\") VALUES ($1, $2)"
    );
    assert_eq!(
        insert.params,
        vec![Value::String("'unsafe".into()), Value::Null]
    );
    assert_eq!(
        Insert::new("users", &["email"]).compile().unwrap_err(),
        QueryError::MissingRequiredField("email")
    );
    assert_eq!(
        Insert::new("users", &[])
            .set(ID, 1)
            .set(ID, 2)
            .compile()
            .unwrap_err(),
        QueryError::DuplicateField
    );
    assert_eq!(
        Insert::new("users", &[])
            .set(OTHER, 1)
            .compile()
            .unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        Insert::new("users", &[]).compile().unwrap().sql,
        "INSERT INTO \"users\" DEFAULT VALUES"
    );
    let update = Update::new("users")
        .set_some(NAME, "Alice")
        .where_(ID.equals(1))
        .compile()
        .unwrap();
    assert_eq!(
        update.sql,
        "UPDATE \"users\" SET \"name\" = $1 WHERE \"id\" = $2"
    );
    assert_eq!(
        update.params,
        vec![Value::String("Alice".into()), Value::Int(1)]
    );
    assert_eq!(
        Update::new("users").set_null(NAME).compile().unwrap_err(),
        QueryError::UnfilteredMutation
    );
    assert_eq!(
        Update::new("users")
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::EmptyMutation
    );
    assert_eq!(
        Delete::new("users").compile().unwrap_err(),
        QueryError::UnfilteredMutation
    );
    let delete = Delete::new("users").where_(ID.equals(7)).compile().unwrap();
    assert_eq!(delete.sql, "DELETE FROM \"users\" WHERE \"id\" = $1");
    assert_eq!(delete.params, vec![Value::Int(7)]);
}

#[test]
fn debug_output_does_not_leak_parameters() {
    let query = users()
        .where_(EMAIL.equals("private@example.org"))
        .compile()
        .unwrap();
    assert!(!format!("{query:?}").contains("private@example.org"));
    assert!(!format!(
        "{:?}",
        users().where_(EMAIL.contains("private@example.org"))
    )
    .contains("private@example.org"));
}

#[test]
fn returning_is_quoted_and_available_for_each_mutation() {
    use rustorm::query::{Delete, Insert, Update};
    let cols = &["id", "name"];
    let q = Insert::new("users", &["email"])
        .set(EMAIL, "a")
        .returning(cols)
        .compile()
        .unwrap();
    assert!(q.sql.ends_with(" RETURNING \"id\", \"name\""));
    let q = Update::new("users")
        .set(EMAIL, "b")
        .where_(ID.equals(1))
        .returning(cols)
        .compile()
        .unwrap();
    assert!(q.sql.ends_with(" RETURNING \"id\", \"name\""));
    let q = Delete::new("users")
        .where_(ID.equals(1))
        .returning(cols)
        .compile()
        .unwrap();
    assert!(q.sql.ends_with(" RETURNING \"id\", \"name\""));
    assert_eq!(
        Delete::new("users")
            .where_(ID.equals(1))
            .returning(&[])
            .compile()
            .unwrap_err(),
        QueryError::EmptySelection
    );
}

#[test]
fn grouped_filters_are_parameterized_validated_and_bounded() {
    use rustorm::query::Filter;
    let query = users()
        .where_(Filter::and(vec![
            ID.gte(2),
            Filter::or(vec![EMAIL.contains("%_\\"), Filter::negate(NAME.is_null())]),
        ]))
        .compile()
        .unwrap();
    assert_eq!(query.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE (\"id\" >= $1 AND (\"email\" LIKE $2 ESCAPE E'\\\\' OR NOT (\"name\" IS NULL)))");
    assert_eq!(
        query.params,
        vec![Value::Int(2), Value::String("%\\%\\_\\\\%".into())]
    );
    assert_eq!(
        users()
            .where_(Filter::or(vec![OTHER.equals(1)]))
            .compile()
            .unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        users().where_(Filter::or(vec![])).compile().unwrap_err(),
        QueryError::EmptyFilterGroup
    );
    assert_eq!(
        rustorm::query::Delete::new("users")
            .where_(Filter::and(vec![]))
            .compile()
            .unwrap_err(),
        QueryError::EmptyFilterGroup
    );
    let mut nested = ID.equals(1);
    for _ in 0..66 {
        nested = Filter::negate(nested);
    }
    assert_eq!(
        users().where_(nested).compile().unwrap_err(),
        QueryError::FilterTooDeep
    );
}

#[test]
fn list_filters_are_typed_parameterized_and_combined() {
    use rustorm::query::{Filter, Update};
    let hostile = "') OR TRUE --";
    let q = users()
        .where_(Filter::or(vec![
            EMAIL.is_in([hostile, "normal"]),
            ID.not_in([1, 2]),
        ]))
        .where_(NAME.not_equals_some("excluded"))
        .take(5)
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE (\"email\" IN ($1, $2) OR \"id\" NOT IN ($3, $4)) AND \"name\" <> $5 LIMIT $6");
    assert_eq!(
        q.params,
        vec![
            Value::String(hostile.into()),
            Value::String("normal".into()),
            Value::Int(1),
            Value::Int(2),
            Value::String("excluded".into()),
            Value::BigInt(5)
        ]
    );
    assert!(!format!("{q:?}").contains(hostile));
    let q = Update::new("users")
        .set(EMAIL, "new")
        .where_(ID.is_in([7, 8]))
        .compile()
        .unwrap();
    assert_eq!(
        q.sql,
        "UPDATE \"users\" SET \"email\" = $1 WHERE \"id\" IN ($2, $3)"
    );
    assert_eq!(
        q.params,
        vec![Value::String("new".into()), Value::Int(7), Value::Int(8)]
    );
    assert_eq!(
        users().where_(OTHER.is_in([1])).compile().unwrap_err(),
        QueryError::WrongModel
    );
}

#[test]
fn lists_reject_empty_null_and_excessive_parameters() {
    use rustorm::query::{Delete, IntoFieldValue, MAX_PARAMETERS};
    assert_eq!(
        users()
            .where_(ID.is_in(Vec::<i32>::new()))
            .compile()
            .unwrap_err(),
        QueryError::EmptyValueList
    );
    assert_eq!(
        Delete::new("users")
            .where_(ID.not_in(Vec::<i32>::new()))
            .compile()
            .unwrap_err(),
        QueryError::EmptyValueList
    );
    struct Null;
    impl IntoFieldValue<i32> for Null {
        fn into_value(self) -> Value {
            Value::Null
        }
    }
    assert_eq!(
        users().where_(ID.is_in([Null])).compile().unwrap_err(),
        QueryError::NullInValueList
    );
    assert_eq!(
        users()
            .where_(ID.is_in(vec![1; MAX_PARAMETERS + 1]))
            .compile()
            .unwrap_err(),
        QueryError::TooManyParameters
    );
    let q = users()
        .where_(ID.is_in(vec![1; MAX_PARAMETERS]))
        .compile()
        .unwrap();
    assert_eq!(q.params.len(), MAX_PARAMETERS);
    assert_eq!(
        users()
            .where_(ID.is_in(vec![1; MAX_PARAMETERS]))
            .take(1)
            .compile()
            .unwrap_err(),
        QueryError::TooManyParameters
    );
    assert_eq!(
        users()
            .where_(ID.is_in(vec![1; MAX_PARAMETERS - 1]))
            .where_(ID.is_in([2, 3]))
            .compile()
            .unwrap_err(),
        QueryError::TooManyParameters
    );
}

#[test]
fn optional_value_comparisons_and_searches_have_sql_null_semantics() {
    let q = users()
        .where_(NAME.in_some(["Alice", "Bob"]))
        .where_(NAME.not_in_some(["x"]))
        .where_(NAME.lt_some("z"))
        .where_(NAME.lte_some("z"))
        .where_(NAME.gt_some("a"))
        .where_(NAME.gte_some("a"))
        .where_(NAME.contains_some("%_\\"))
        .where_(NAME.starts_with_some("%"))
        .where_(NAME.ends_with_some("_"))
        .where_(EMAIL.not_equals("none"))
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE \"name\" IN ($1, $2) AND \"name\" NOT IN ($3) AND \"name\" < $4 AND \"name\" <= $5 AND \"name\" > $6 AND \"name\" >= $7 AND \"name\" LIKE $8 ESCAPE E'\\\\' AND \"name\" LIKE $9 ESCAPE E'\\\\' AND \"name\" LIKE $10 ESCAPE E'\\\\' AND \"email\" <> $11");
    assert_eq!(
        q.params[7..10],
        [
            Value::String("%\\%\\_\\\\%".into()),
            Value::String("\\%%".into()),
            Value::String("%\\_".into())
        ]
    );
}

#[test]
fn count_builds_a_typed_aggregate_with_validated_filters() {
    use rustorm::query::{Count, CountResult, Filter, MAX_PARAMETERS};
    use rustorm::row::{Column, FromRow, Row};
    let q = Count::new("weird\"table").compile().unwrap();
    assert_eq!(
        q.sql,
        "SELECT COUNT(*) AS \"count\" FROM \"weird\"\"table\""
    );
    assert!(q.params.is_empty());
    let q = Count::new("users")
        .where_(Filter::or(vec![
            ID.equals(4),
            EMAIL.is_in(["bad';--", "b"]),
        ]))
        .where_(NAME.is_null())
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT COUNT(*) AS \"count\" FROM \"users\" WHERE (\"id\" = $1 OR \"email\" IN ($2, $3)) AND \"name\" IS NULL");
    assert_eq!(
        q.params,
        vec![
            Value::Int(4),
            Value::String("bad';--".into()),
            Value::String("b".into())
        ]
    );
    assert_eq!(
        Count::new("users")
            .where_(OTHER.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        Count::new("users")
            .where_(ID.is_in(Vec::<i32>::new()))
            .compile()
            .unwrap_err(),
        QueryError::EmptyValueList
    );
    assert_eq!(
        Count::new("users")
            .where_(ID.is_in(vec![1; MAX_PARAMETERS]))
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::TooManyParameters
    );
    assert_eq!(
        Count::new("users").distinct().compile().unwrap_err(),
        QueryError::DistinctCountNeedsField
    );
    assert_eq!(
        Count::new("users").field(OTHER).compile().unwrap_err(),
        QueryError::WrongModel
    );
    let q = Count::new("users")
        .field(NAME)
        .distinct()
        .where_(ID.gte(5))
        .compile()
        .unwrap();
    assert_eq!(
        q.sql,
        "SELECT COUNT(DISTINCT \"name\") AS \"count\" FROM \"users\" WHERE \"id\" >= $1"
    );
    assert_eq!(q.params, vec![Value::Int(5)]);
    assert_eq!(
        Count::new("users").field(NAME).compile().unwrap().sql,
        "SELECT COUNT(\"name\") AS \"count\" FROM \"users\""
    );
    let row = Row::new(vec![Column {
        name: "count".into(),
        oid: 20,
        value: Some("42".into()),
    }]);
    assert_eq!(
        CountResult::from_row(&row).unwrap(),
        CountResult { count: 42 }
    );
    assert!(CountResult::from_row(&Row::new(vec![])).is_err());
}

#[test]
fn optional_order_supports_explicit_null_position() {
    let q = users()
        .order_by(NAME.asc_nulls_first())
        .order_by(NAME.asc_nulls_last())
        .order_by(NAME.desc_nulls_first())
        .order_by(NAME.desc_nulls_last())
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" ORDER BY \"name\" ASC NULLS FIRST, \"name\" ASC NULLS LAST, \"name\" DESC NULLS FIRST, \"name\" DESC NULLS LAST");
    let other = Field::<Option<String>>::new("posts", "name");
    assert_eq!(
        users()
            .order_by(other.asc_nulls_first())
            .compile()
            .unwrap_err(),
        QueryError::WrongModel
    );
}

#[test]
fn atomic_arithmetic_uses_parameters_and_checks_preconditions() {
    use rustorm::query::Update;
    const COUNT: Field<i32> = Field::new("users", "count");
    const BIG: Field<i64> = Field::new("users", "big");
    const FLOAT: Field<f64> = Field::new("users", "float");
    const OPTIONAL: Field<Option<i32>> = Field::new("users", "optional");
    let q = Update::new("users")
        .increment(COUNT, 2)
        .decrement(BIG, 3_i64)
        .multiply(FLOAT, 1.5)
        .divide_some(OPTIONAL, 2)
        .where_(ID.equals(4))
        .returning(&["id"])
        .compile()
        .unwrap();
    assert_eq!(q.sql, "UPDATE \"users\" SET \"count\" = \"count\" + $1, \"big\" = \"big\" - $2, \"float\" = \"float\" * $3, \"optional\" = \"optional\" / $4 WHERE \"id\" = $5 RETURNING \"id\"");
    assert_eq!(
        q.params,
        vec![
            Value::Int(2),
            Value::BigInt(3),
            Value::Float(1.5),
            Value::Int(2),
            Value::Int(4)
        ]
    );
    assert_eq!(
        Update::new("users")
            .increment(COUNT, 1)
            .compile()
            .unwrap_err(),
        QueryError::UnfilteredMutation
    );
    assert_eq!(
        Update::new("users")
            .increment(OTHER, 1)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        Update::new("users")
            .increment(COUNT, 1)
            .set(COUNT, 2)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::DuplicateField
    );
    assert_eq!(
        Update::new("users")
            .divide(COUNT, 0)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::DivisionByZero
    );
    assert_eq!(
        Update::new("users")
            .divide(BIG, 0_i64)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::DivisionByZero
    );
    assert_eq!(
        Update::new("users")
            .divide(FLOAT, -0.0)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::DivisionByZero
    );
    assert_eq!(
        Update::new("users")
            .multiply(FLOAT, f64::NAN)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::NonFiniteArithmeticOperand
    );
    assert_eq!(
        Update::new("users")
            .increment(FLOAT, f64::INFINITY)
            .where_(ID.equals(1))
            .compile()
            .unwrap_err(),
        QueryError::NonFiniteArithmeticOperand
    );
    let q = Update::new("users")
        .increment_some(OPTIONAL, 1)
        .where_(ID.equals(1))
        .compile()
        .unwrap();
    assert_eq!(
        q.sql,
        "UPDATE \"users\" SET \"optional\" = \"optional\" + $1 WHERE \"id\" = $2"
    );
    let q = Update::new("users")
        .decrement_some(OPTIONAL, 1)
        .where_(ID.equals(1))
        .compile()
        .unwrap();
    assert!(q.sql.contains("\"optional\" - $1"));
    let q = Update::new("users")
        .multiply_some(OPTIONAL, 1)
        .where_(ID.equals(1))
        .compile()
        .unwrap();
    assert!(q.sql.contains("\"optional\" * $1"));
}

#[test]
fn case_insensitive_search_is_literal_and_parameterized() {
    let q = users()
        .where_(EMAIL.contains_insensitive("%_\\"))
        .where_(EMAIL.starts_with_insensitive("A%"))
        .where_(EMAIL.ends_with_insensitive("B_"))
        .where_(NAME.contains_insensitive_some("secret"))
        .where_(NAME.starts_with_insensitive_some("x"))
        .where_(NAME.ends_with_insensitive_some("z"))
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\", \"email\", \"name\" FROM \"users\" WHERE \"email\" ILIKE $1 ESCAPE E'\\\\' AND \"email\" ILIKE $2 ESCAPE E'\\\\' AND \"email\" ILIKE $3 ESCAPE E'\\\\' AND \"name\" ILIKE $4 ESCAPE E'\\\\' AND \"name\" ILIKE $5 ESCAPE E'\\\\' AND \"name\" ILIKE $6 ESCAPE E'\\\\'");
    assert_eq!(
        q.params,
        vec![
            Value::String("%\\%\\_\\\\%".into()),
            Value::String("A\\%%".into()),
            Value::String("%B\\_".into()),
            Value::String("%secret%".into()),
            Value::String("x%".into()),
            Value::String("%z".into())
        ]
    );
    assert!(!format!("{q:?}").contains("secret"));
}

#[test]
fn projections_and_distinct_validate_model_and_sort_fields() {
    let q = users()
        .select(NAME)
        .select(EMAIL)
        .distinct()
        .where_(ID.gte(1))
        .order_by(EMAIL.asc())
        .take(3)
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT DISTINCT \"name\", \"email\" FROM \"users\" WHERE \"id\" >= $1 ORDER BY \"email\" ASC LIMIT $2");
    assert_eq!(q.params, vec![Value::Int(1), Value::BigInt(3)]);
    assert_eq!(
        users().select(OTHER).compile().unwrap_err(),
        QueryError::WrongModel
    );
    assert_eq!(
        users().select(ID).select(ID).compile().unwrap_err(),
        QueryError::DuplicateField
    );
    assert_eq!(
        users()
            .select(EMAIL)
            .distinct()
            .order_by(ID.asc())
            .compile()
            .unwrap_err(),
        QueryError::DistinctOrderNotSelected
    );
    assert!(users().select(EMAIL).order_by(ID.asc()).compile().is_ok());
    assert_eq!(
        Select::new("users", &[]).select(ID).compile().unwrap().sql,
        "SELECT \"id\" FROM \"users\""
    );
}
