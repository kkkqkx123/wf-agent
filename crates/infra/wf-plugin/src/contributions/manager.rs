use std::sync::Arc;
use std::sync::RwLock;

use serde_json::Value;
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::MiddlewarePhase;
use wf_types::SystemPromptFragment;
use wf_types::Template;

use super::registrar::ContributionRegistrar;
use super::registries::{MultiRegistry, Registry};
use super::types::*;
use crate::error::{PluginError, PluginResult};

#[path = "manager/registry.rs"]
mod registry;
#[path = "manager/node_tool.rs"]
mod node_tool;
#[path = "manager/llm.rs"]
mod llm;
#[path = "manager/content.rs"]
mod content;
#[path = "manager/middleware.rs"]
mod middleware;
#[path = "manager/registrar.rs"]
mod registrar;

pub use registry::*;
pub use registrar::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::types::NextFn;

    struct NoopMiddlewareHandler;

    #[async_trait::async_trait]
    impl PluginMiddlewareHandler for NoopMiddlewareHandler {
        async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
            next(context).await
        }
    }

    /// Records the context it receives, then passes a rewrite downstream.
    struct RewriteMiddlewareHandler {
        seen: Arc<std::sync::Mutex<Vec<Value>>>,
        replacement: Value,
        call_next: bool,
    }

    #[async_trait::async_trait]
    impl PluginMiddlewareHandler for RewriteMiddlewareHandler {
        async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
            self.seen.lock().expect("seen lock poisoned").push(context);
            if self.call_next {
                next(self.replacement.clone()).await
            } else {
                Ok(self.replacement.clone())
            }
        }
    }

    fn register_for_run(
        manager: &ContributionManager,
        phase: &MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) {
        manager.start_registration("mw-test");
        manager
            .as_registrar()
            .register_middleware(phase.clone(), priority, handler)
            .expect("middleware registers");
    }

    #[test]
    fn middleware_phase_round_trips_through_registry() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let mut registrar = manager.as_registrar();
        registrar
            .register_middleware(
                MiddlewarePhase::OnCheckpoint,
                10,
                Arc::new(NoopMiddlewareHandler),
            )
            .unwrap();
        assert_eq!(
            manager.get_middleware(&MiddlewarePhase::OnCheckpoint).len(),
            1
        );
        assert_eq!(
            manager
                .get_middleware(&MiddlewarePhase::from("on-checkpoint"))
                .len(),
            1
        );
        assert!(manager
            .get_middleware(&MiddlewarePhase::Other("custom-phase".into()))
            .is_empty());
    }

    #[tokio::test]
    async fn middleware_rewrite_threads_through_chain() {
        use serde_json::json;

        let manager = ContributionManager::new();
        let phase = MiddlewarePhase::from("rewrite-phase");
        let outer_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let inner_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        register_for_run(
            &manager,
            &phase,
            10,
            Arc::new(RewriteMiddlewareHandler {
                seen: outer_seen.clone(),
                replacement: json!({"stage": "outer"}),
                call_next: true,
            }),
        );
        register_for_run(
            &manager,
            &phase,
            5,
            Arc::new(RewriteMiddlewareHandler {
                seen: inner_seen.clone(),
                replacement: json!({"stage": "inner"}),
                call_next: true,
            }),
        );

        let out = manager
            .run_middleware(&phase, json!({"stage": "start"}))
            .await
            .expect("chain runs");
        assert_eq!(out, json!({"stage": "inner"}));
        assert_eq!(
            *outer_seen.lock().expect("lock"),
            vec![json!({"stage": "start"})]
        );
        assert_eq!(
            *inner_seen.lock().expect("lock"),
            vec![json!({"stage": "outer"})]
        );
    }

    #[tokio::test]
    async fn middleware_short_circuit_skips_downstream() {
        use serde_json::json;

        let manager = ContributionManager::new();
        let phase = MiddlewarePhase::from("short-phase");
        let inner_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        register_for_run(
            &manager,
            &phase,
            10,
            Arc::new(RewriteMiddlewareHandler {
                seen: Arc::new(std::sync::Mutex::new(Vec::new())),
                replacement: json!({"stopped": true}),
                call_next: false,
            }),
        );
        register_for_run(
            &manager,
            &phase,
            5,
            Arc::new(RewriteMiddlewareHandler {
                seen: inner_seen.clone(),
                replacement: json!({"never": true}),
                call_next: true,
            }),
        );

        let out = manager
            .run_middleware(&phase, json!({"stage": "start"}))
            .await
            .expect("chain runs");
        assert_eq!(out, json!({"stopped": true}));
        assert!(inner_seen.lock().expect("lock").is_empty());
    }

    #[test]
    fn resource_contributions_round_trip_by_owner() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_prompt(
                "sys.plugin",
                Template {
                    id: "sys.plugin".into(),
                    name: "plugin prompt".into(),
                    description: None,
                    category: "system".into(),
                    content: "hello from plugin".into(),
                    variables: None,
                    fragments: None,
                },
            )
            .unwrap();
        manager
            .as_registrar()
            .register_fragment(
                "fragments.plugin.rule",
                SystemPromptFragment {
                    id: "fragments.plugin.rule".into(),
                    category: "constraint".into(),
                    content: "plugin constraint".into(),
                    description: None,
                    variables: None,
                },
            )
            .unwrap();

        // Registered under the owning plugin, queryable by type.
        assert_eq!(
            manager.all_prompts(),
            vec![("sys.plugin".to_string(), "p1".to_string())]
        );
        assert!(manager.get_prompt("sys.plugin").is_some());
        assert!(manager.get_fragment("fragments.plugin.rule").is_some());
        assert!(manager
            .contributions_for("p1")
            .contains(&("prompt".to_string(), "sys.plugin".to_string())));
        assert!(manager
            .contributions_for("p1")
            .contains(&("fragment".to_string(), "fragments.plugin.rule".to_string())));

        // Override policy `Forbid` keeps the original owner and reports the
        // conflict to the caller: p2 cannot claim the same id, so ownership
        // stays with p1 and the registration returns an error.
        manager.start_registration("p2");
        let err = manager
            .as_registrar()
            .register_prompt(
                "sys.plugin",
                Template {
                    id: "sys.plugin".into(),
                    name: "attempted override".into(),
                    description: None,
                    category: "system".into(),
                    content: "nope".into(),
                    variables: None,
                    fragments: None,
                },
            )
            .unwrap_err();
        assert!(matches!(err, PluginError::ContributionConflict(_)));
        assert_eq!(
            manager.all_prompts(),
            vec![("sys.plugin".to_string(), "p1".to_string())]
        );

        // Unregistering the owner clears every resource contribution.
        manager.unregister_all("p1");
        assert!(manager.all_prompts().is_empty());
        assert!(manager.all_fragments().is_empty());
        assert!(manager
            .contributions_for("p1")
            .iter()
            .all(|(t, _)| t != "prompt" && t != "fragment"));
    }

    #[test]
    fn allow_policy_overrides_previous_owner() {
        let manager = ContributionManager::new();
        manager.set_override_policy(OverridePolicy::Allow);
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_tool(
                "shared-tool",
                wf_types::tool::Tool {
                    id: "shared-tool".into(),
                    name: "v1".into(),
                    description: String::new(),
                    tool_type: wf_types::tool::ToolType::BuiltIn,
                    parameters: None,
                    metadata: None,
                    config: None,
                    enabled: None,
                    strict: None,
                    default_timeout_ms: None,
                },
            )
            .unwrap();
        manager.start_registration("p2");
        manager
            .as_registrar()
            .register_tool(
                "shared-tool",
                wf_types::tool::Tool {
                    id: "shared-tool".into(),
                    name: "v2".into(),
                    description: String::new(),
                    tool_type: wf_types::tool::ToolType::BuiltIn,
                    parameters: None,
                    metadata: None,
                    config: None,
                    enabled: None,
                    strict: None,
                    default_timeout_ms: None,
                },
            )
            .unwrap();
        assert_eq!(
            manager.all_tools(),
            vec![("shared-tool".to_string(), "p2".to_string())]
        );
    }

    #[test]
    fn invalid_keys_are_reported_not_dropped() {
        struct NoopTool;
        #[async_trait::async_trait]
        impl crate::contributions::types::PluginToolExecutor for NoopTool {
            async fn execute(
                &self,
                _ctx: crate::contributions::types::PluginToolContext,
            ) -> PluginResult<crate::contributions::types::PluginToolResult> {
                Ok(crate::contributions::types::PluginToolResult {
                    result: Value::Null,
                })
            }
        }
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let err = manager
            .as_registrar()
            .register_tool_type("  ", Arc::new(NoopTool))
            .unwrap_err();
        assert!(matches!(err, PluginError::InvalidContribution { .. }));
        assert!(manager.all_tool_types().is_empty());
    }

    #[test]
    fn llm_codec_registers_custom_format() {
        struct NoopCodec;
        impl crate::contributions::types::PluginLlmCodec for NoopCodec {
            fn build_request(
                &self,
                _request: Value,
                _profile: Value,
            ) -> PluginResult<crate::contributions::types::CodecHttpRequest> {
                Err(PluginError::Internal("noop".to_string()))
            }
            fn parse_response(&self, _body: &str, _request: Value) -> PluginResult<Value> {
                Err(PluginError::Internal("noop".to_string()))
            }
        }
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider("acme", Arc::new(NoopCodec))
            .unwrap();

        assert!(manager.get_llm_codec("acme").is_some());
        assert_eq!(
            manager.all_llm_providers(),
            vec![("acme".to_string(), "p1".to_string())]
        );
        assert!(manager
            .contributions_for("p1")
            .contains(&("llm-provider".to_string(), "acme".to_string())));

        manager
            .as_registrar()
            .register_llm_provider_definition(wf_types::llm::LlmProviderDefinition {
                id: "acme".to_string(),
                name: None,
                description: None,
                base_url: Some("https://api.acme.test".to_string()),
                auth_type: Some("bearer".to_string()),
                default_headers: None,
                format: wf_types::llm::LlmFormat::Custom("ACME_CHAT".to_string()),
                model_discovery: None,
                api_version: None,
                metadata: None,
                proxy: None,
                no_proxy: None,
                rate_limit: None,
            })
            .unwrap();
        assert_eq!(
            manager.all_llm_provider_definitions(),
            vec![("acme".to_string(), "p1".to_string())]
        );

        manager.unregister_all("p1");
        assert!(manager.all_llm_providers().is_empty());
        assert!(manager.all_llm_provider_definitions().is_empty());
    }
}