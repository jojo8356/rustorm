use rustorm::extensions::{ClientExtensions, QueryContext};

#[test]
fn middleware_intercepts() {
    let ext = ClientExtensions::new();
    ext.use_middleware(|ctx, next| {
        if ctx.operation == "findMany" { Ok("intercepted".into()) } else { next(ctx) }
    });
    let ctx = QueryContext { model: "User".into(), operation: "findMany".into(), sql: "SELECT".into() };
    assert_eq!(ext.run(ctx).unwrap(), "intercepted");
}

#[test]
fn middleware_chain_order() {
    let ext = ClientExtensions::new();
    ext.use_middleware(|ctx, next| { let mut r = next(ctx); r.map(|s| format!("a-{s}")) });
    ext.use_middleware(|ctx, _| Ok(format!("b-{}", ctx.model)));
    let ctx = QueryContext { model: "Post".into(), operation: "findMany".into(), sql: "".into() };
    let res = ext.run(ctx).unwrap();
    assert_eq!(res, "a-b-Post");
    assert_eq!(ext.middleware_count(), 2);
}

#[test]
fn extension_computed_field() {
    let ext = ClientExtensions::new();
    ext.add_computed_field("User", "fullName", vec!["firstName".into(), "lastName".into()], |map| {
        format!("{} {}", map.get("firstName").unwrap_or(&"".into()), map.get("lastName").unwrap_or(&"".into()))
    });
    // just ensure it doesn't panic
    assert_eq!(ext.middleware_count(), 0);
}
