use std::collections::HashMap;

use serde_json::Value;

use crate::hooks::types::BaseHookContext;

pub trait HookContextBuilder: Send + Sync {
    fn build_context(&self, hook_ctx: &BaseHookContext) -> HashMap<String, Value>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::Id;

    struct TestContextBuilder;

    impl HookContextBuilder for TestContextBuilder {
        fn build_context(&self, hook_ctx: &BaseHookContext) -> HashMap<String, Value> {
            let mut ctx = HashMap::new();
            ctx.insert(
                "execution_id".to_string(),
                Value::String(hook_ctx.execution_id.clone()),
            );
            for (k, v) in &hook_ctx.data {
                ctx.insert(k.clone(), v.clone());
            }
            ctx
        }
    }

    #[test]
    fn test_build_context() {
        let builder = TestContextBuilder;
        let mut data = HashMap::new();
        data.insert("key".to_string(), Value::String("value".to_string()));

        let hook_ctx = BaseHookContext {
            execution_id: Id::new(),
            data,
        };

        let ctx = builder.build_context(&hook_ctx);
        assert!(ctx.contains_key("execution_id"));
        assert_eq!(ctx.get("key").unwrap(), &Value::String("value".to_string()));
    }
}
