use rustorm::query::{Aggregate, Filter, Field};

fn user_table_fields() -> (Field<i32>, Field<String>) {
    let id = Field { table: "users", name: "id", marker: std::marker::PhantomData };
    let email = Field { table: "users", name: "email", marker: std::marker::PhantomData };
    (id, email)
}

#[test]
fn aggregate_count() {
    let (id, _) = user_table_fields();
    let q = Aggregate::new("users").count(Some(id)).compile().unwrap();
    assert!(q.sql.contains("COUNT"), "{}", q.sql);
    assert!(q.sql.contains("users"), "{}", q.sql);
}

#[test]
fn aggregate_sum_avg_min_max() {
    let (id, _) = user_table_fields();
    let q = Aggregate::new("users").sum(id.clone()).avg(id.clone()).min(id.clone()).max(id).compile().unwrap();
    assert!(q.sql.contains("SUM") && q.sql.contains("AVG") && q.sql.contains("MIN") && q.sql.contains("MAX"), "{}", q.sql);
}

#[test]
fn group_by_with_having() {
    let (id, email) = user_table_fields();
    let filter = email.equals("a@b.c".to_string());
    let q = Aggregate::new("users").count(None::<Field<i32>>).group_by(email.clone()).having(filter).compile().unwrap();
    assert!(q.sql.contains("GROUP BY") && q.sql.contains("HAVING"), "{}", q.sql);
}

#[test]
fn group_by_order_take_skip() {
    let (_, email) = user_table_fields();
    let order = email.asc();
    let q = Aggregate::new("users").count(None::<Field<i32>>).group_by(email).order_by(order).take(10).skip(5).compile().unwrap();
    assert!(q.sql.contains("ORDER BY") && q.sql.contains("LIMIT") && q.sql.contains("OFFSET"), "{}", q.sql);
}
