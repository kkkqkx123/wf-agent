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

#[path = "manager/content.rs"]
mod content;
#[path = "manager/llm.rs"]
mod llm;
#[path = "manager/middleware.rs"]
mod middleware;
#[path = "manager/node_tool.rs"]
mod node_tool;
#[path = "manager/registrar.rs"]
mod registrar;
#[path = "manager/registry.rs"]
mod registry;

pub use registrar::*;
pub use registry::*;

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

    struct StubExtractor;

    impl PluginMetadataExtractor for StubExtractor {
        fn name(&self) -> String {
            "stub-extractor".to_string()
        }

        fn extract(&self, _text: String, _options: Value) -> PluginResult<Value> {
            Ok(Value::Null)
        }
    }

    struct StubSplitter;

    impl PluginChunkSplitter for StubSplitter {
        fn name(&self) -> String {
            "stub-splitter".to_string()
        }

        fn split(&self, _text: String, _options: Value) -> PluginResult<Value> {
            Ok(Value::Null)
        }
    }

    struct StubSelector;

    impl PluginRouteSelector for StubSelector {
        fn select(&self, input: Value) -> PluginResult<Value> {
            Ok(input)
        }
    }

    struct StubPostprocessor;

    impl PluginPostprocessor for StubPostprocessor {
        fn process(&self, input: Value) -> PluginResult<Value> {
            Ok(input)
        }
    }

    struct StubJudge;

    impl PluginEvalJudge for StubJudge {
        fn judge(&self, input: Value) -> PluginResult<Value> {
            Ok(input)
        }
    }

    #[test]
    fn retrieval_extension_registries_round_trip_and_unsync() {
        let manager = ContributionManager::new();
        manager.start_registration("p-deferred");
        let mut registrar = manager.as_registrar();
        registrar
            .register_metadata_extractor("stub-extractor", Arc::new(StubExtractor))
            .unwrap();
        registrar
            .register_chunk_splitter("stub-splitter", Arc::new(StubSplitter))
            .unwrap();
        registrar
            .register_route_selector("stub-selector", Arc::new(StubSelector))
            .unwrap();
        registrar
            .register_postprocessor("stub-post", Arc::new(StubPostprocessor))
            .unwrap();
        registrar
            .register_eval_judge("stub-judge", Arc::new(StubJudge))
            .unwrap();
        assert!(manager.get_metadata_extractor("stub-extractor").is_some());
        assert!(manager.get_chunk_splitter("stub-splitter").is_some());
        assert!(manager.get_route_selector("stub-selector").is_some());
        assert!(manager.get_postprocessor("stub-post").is_some());
        assert!(manager.get_eval_judge("stub-judge").is_some());
        assert_eq!(manager.all_metadata_extractors().len(), 1);
        assert_eq!(manager.all_chunk_splitters().len(), 1);
        assert_eq!(manager.all_route_selectors().len(), 1);
        assert_eq!(manager.all_postprocessors().len(), 1);
        assert_eq!(manager.all_eval_judges().len(), 1);
        manager.unregister_all("p-deferred");
        assert!(manager.get_metadata_extractor("stub-extractor").is_none());
        assert!(manager.get_chunk_splitter("stub-splitter").is_none());
        assert!(manager.get_route_selector("stub-selector").is_none());
        assert!(manager.get_postprocessor("stub-post").is_none());
        assert!(manager.get_eval_judge("stub-judge").is_none());
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
                auth: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
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

    fn cli_command(id: &str, slash: &str) -> PluginCliCommand {
        PluginCliCommand {
            id: id.into(),
            title: "Review".into(),
            group: "Review".into(),
            description: "Review the diff".into(),
            slash_name: Some(slash.into()),
            slash_aliases: Vec::new(),
            slash_argument: String::new(),
            palette: true,
            key_hint: None,
            when: PluginCliCondition::default(),
        }
    }

    #[test]
    fn cli_command_registers_and_conflicts_forbid_reclaim() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_cli_command(cli_command("review.run", "review"))
            .unwrap();
        assert_eq!(
            manager.all_cli_commands(),
            vec![("review.run".to_string(), "p1".to_string())]
        );
        assert!(manager
            .contributions_for("p1")
            .contains(&("cli-command".to_string(), "review.run".to_string())));

        manager.start_registration("p2");
        let err = manager
            .as_registrar()
            .register_cli_command(cli_command("review.run", "review"))
            .unwrap_err();
        assert!(matches!(err, PluginError::ContributionConflict(_)));
        assert_eq!(
            manager.get_cli_command("review.run").expect("kept").title,
            "Review"
        );

        manager.unregister_all("p1");
        assert!(manager.all_cli_commands().is_empty());
        manager
            .as_registrar()
            .register_cli_command(cli_command("review.run", "review"))
            .unwrap();
        assert_eq!(
            manager.all_cli_commands(),
            vec![("review.run".to_string(), "p2".to_string())]
        );
    }

    #[test]
    fn cli_command_validation_rejects_unroutable_definitions() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let mut bad = cli_command("review.run", "review");
        bad.slash_name = None;
        let err = manager
            .as_registrar()
            .register_cli_command(bad)
            .unwrap_err();
        assert!(matches!(err, PluginError::InvalidContribution { .. }));
        assert!(manager.all_cli_commands().is_empty());
    }

    #[test]
    fn cli_surrogates_round_trip_and_unregister_by_owner() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_completion_provider(PluginCompletionSource {
                id: "issues".into(),
                trigger_chars: vec!["#".into()],
                items: vec![PluginCompletionItem {
                    value: "#12".into(),
                    label: None,
                    description: None,
                }],
            })
            .unwrap();
        manager
            .as_registrar()
            .register_setting_field(PluginSettingField {
                key: "review.style".into(),
                label: "Style".into(),
                values: vec!["short".into(), "full".into()],
                fallback: "short".into(),
            })
            .unwrap();
        manager
            .as_registrar()
            .register_tool_summary(PluginToolSummary {
                tool_name: "grep".into(),
                call_template: "search {args}".into(),
                result_template: String::new(),
            })
            .unwrap();
        manager
            .as_registrar()
            .register_markdown_transformer(PluginMarkdownRule {
                id: "arrows".into(),
                pattern: "-->".into(),
                replacement: "→".into(),
                surfaces: vec![],
            })
            .unwrap();

        assert_eq!(manager.completion_provider_snapshot().len(), 1);
        assert_eq!(manager.setting_field_snapshot().len(), 1);
        assert!(manager.get_tool_summary("grep").is_some());
        assert_eq!(manager.markdown_rules_for("assistant").len(), 1);
        assert!(manager.markdown_rules_for("user").len() == 1);

        manager.unregister_all("p1");
        assert!(manager.all_completion_providers().is_empty());
        assert!(manager.all_setting_fields().is_empty());
        assert!(manager.all_tool_summaries().is_empty());
        assert!(manager.all_markdown_transformers().is_empty());
    }
}
