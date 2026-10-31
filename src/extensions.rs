//! P2: Extensions and middlewares for client
use std::sync::{Arc, Mutex};

pub struct QueryContext {
    pub model: String,
    pub operation: String,
    pub sql: String,
}

pub type Next = Arc<dyn Fn(QueryContext) -> Result<String, String> + Send + Sync>;
pub type Middleware = Arc<dyn Fn(QueryContext, Next) -> Result<String, String> + Send + Sync>;

#[derive(Clone)]
pub struct ClientExtensions {
    middlewares: Arc<Mutex<Vec<Middleware>>>,
    computed_fields: Arc<Mutex<Vec<ComputedField>>>,
}

pub struct ComputedField {
    pub model: String,
    pub field: String,
    pub needs: Vec<String>,
    pub compute: Arc<dyn Fn(&std::collections::HashMap<String,String>) -> String + Send + Sync>,
}

impl ClientExtensions {
    pub fn new() -> Self {
        Self { middlewares: Arc::new(Mutex::new(vec![])), computed_fields: Arc::new(Mutex::new(vec![])) }
    }
    pub fn use_middleware<F>(&self, f: F) where F: Fn(QueryContext, Next) -> Result<String,String> + Send + Sync + 'static {
        self.middlewares.lock().unwrap().push(Arc::new(f));
    }
    pub fn add_computed_field<F>(&self, model: &str, field: &str, needs: Vec<String>, compute: F)
    where F: Fn(&std::collections::HashMap<String,String>) -> String + Send + Sync + 'static {
        self.computed_fields.lock().unwrap().push(ComputedField { model: model.to_string(), field: field.to_string(), needs, compute: Arc::new(compute) });
    }
    pub fn run(&self, ctx: QueryContext) -> Result<String, String> {
        let mws = self.middlewares.lock().unwrap().clone();
        fn chain(idx: usize, mws: Vec<Middleware>, ctx: QueryContext) -> Result<String,String> {
            if idx >= mws.len() {
                Ok(format!("executed:{}:{}", ctx.model, ctx.operation))
            } else {
                let mws_clone = mws.clone();
                let next = Arc::new(move |c: QueryContext| chain(idx+1, mws_clone.clone(), c)) as Next;
                let mw = mws[idx].clone();
                mw(ctx, next)
            }
        }
        chain(0, mws, ctx)
    }
    pub fn middleware_count(&self) -> usize { self.middlewares.lock().unwrap().len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn middleware_intercepts() {
        let ext = ClientExtensions::new();
        ext.use_middleware(|ctx, next| {
            if ctx.operation == "findMany" { Ok("intercepted".into()) } else { next(ctx) }
        });
        let ctx = QueryContext { model: "User".into(), operation: "findMany".into(), sql: "SELECT".into() };
        assert_eq!(ext.run(ctx).unwrap(), "intercepted");
    }
}
