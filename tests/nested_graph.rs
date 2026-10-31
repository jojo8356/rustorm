use rustorm::{
    query::{Field, Insert, Update, Delete, Filter, QueryError, Value},
    nested_writes::{NestedGraph, RelationRef},
};
const P_ID: Field<i32> = Field::new("parents","id");
const C_ID: Field<i32> = Field::new("children","id");
const C_TITLE: Field<String> = Field::new("children","title");

#[test]
fn graph_validates_contradictions_and_cycles() {
    let mut g = NestedGraph::new();
    assert!(matches!(g.validate(), Err(QueryError::EmptyNestedWrite)));

    // create parent
    let p = Insert::new("parents", &["id"]).set(P_ID, 1);
    let a = g.add_create(p).unwrap();
    // create child via relation
    let rel = RelationRef { parent_table: "parents", parent_key: "id", child_table: "children", child_key: "parent_id", nullable: true };
    let c = Insert::new("children", &["id"]).set(C_ID, 10);
    let b = g.add_create_child(rel, c).unwrap();
    g.add_dependency(a, b).unwrap();
    assert!(g.validate().is_ok());

    // contradictory connect/disconnect on same pair
    let mut g2 = NestedGraph::new();
    let pv = Value::Int(1);
    let cv = Value::Int(10);
    g2.add_connect_through("links","a","b", pv.clone(), cv.clone()).unwrap();
    assert!(matches!(g2.add_disconnect_through("links","a","b", pv, cv), Err(QueryError::EmptyFilterGroup)));
}

#[test]
fn graph_cycle_detection() {
    let mut g = NestedGraph::new();
    let p1 = Insert::new("parents",&["id"]).set(P_ID,1);
    let p2 = Insert::new("parents",&["id"]).set(P_ID,2);
    let a = g.add_create(p1).unwrap();
    let b = g.add_create(p2).unwrap();
    g.add_dependency(a,b).unwrap();
    // créer un cycle b -> a
    assert!(matches!(g.add_dependency(b,a), Err(QueryError::EmptyFilterGroup)));
    // ou via validate si on force
    let mut g3 = NestedGraph::new();
    let x = g3.add_create(Insert::new("parents",&["id"]).set(P_ID,1)).unwrap();
    let y = g3.add_create(Insert::new("parents",&["id"]).set(P_ID,2)).unwrap();
    let z = g3.add_create(Insert::new("parents",&["id"]).set(P_ID,3)).unwrap();
    g3.add_dependency(x,y).unwrap();
    g3.add_dependency(y,z).unwrap();
    g3.add_dependency(z,x).unwrap();
    assert!(matches!(g3.validate(), Err(QueryError::EmptyFilterGroup)));
}

#[test]
fn graph_operations_atomicity() {
    let mut g = NestedGraph::new();
    // mix of operations
    g.add_create(Insert::new("parents",&["id"]).set(P_ID, 1)).unwrap();
    g.add_update("children", C_ID.equals(10), Update::new("children").set(C_TITLE, "new".to_owned())).unwrap();
    g.add_delete("children", C_ID.equals(20), Delete::new("children")).unwrap();
    g.add_upsert("children", C_ID.equals(30), Insert::new("children",&["id"]).set(C_ID,30), Update::new("children").set(C_TITLE,"up".to_owned())).unwrap();
    g.add_connect_or_create("children", C_ID.equals(40), Insert::new("children",&["id"]).set(C_ID,40), "parent_id", Value::Int(1)).unwrap();
    g.add_create_child(RelationRef { parent_table:"parents", parent_key:"id", child_table:"children", child_key:"parent_id", nullable:true }, Insert::new("children",&["id"]).set(C_ID,50)).unwrap();
    assert!(g.validate().is_ok());
    assert_eq!(g.execution_order().len(), 6);
}

#[test]
fn graph_multi_level_dependencies() {
    // Simule création User -> Post -> Tag (3 niveaux)
    let mut g = NestedGraph::new();
    let user = g.add_create(Insert::new("users",&["id"]).set(Field::<i32>::new("users","id"), 1)).unwrap();
    let post_rel = RelationRef { parent_table:"users", parent_key:"id", child_table:"posts", child_key:"author_id", nullable:false };
    let post = g.add_create_child(post_rel, Insert::new("posts",&["id"]).set(Field::<i32>::new("posts","id"), 10)).unwrap();
    g.add_dependency(user, post).unwrap();
    // Tag via N-N explicite
    g.add_connect_through("post_tags","post_id","tag_id", Value::Int(10), Value::String("rust".into())).unwrap();
    assert!(g.validate().is_ok());
}
