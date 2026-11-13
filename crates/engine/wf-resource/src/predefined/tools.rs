//! Predefined tool registration.
//!
//! The tool definitions (schema + description) are declared once in
//! `wf-tools::predefined`; this module only registers them into the
//! shared `ToolRegistry`.
//!
//! Registration also applies capability visibility: a tool whose service
//! subsystem is confirmed unusable is not registered, so the model never
//! spends a turn on a call that must fail. The predicate reads the live
//! service state, so a recovered service becomes visible to the next
//! execution without a restart.

use wf_core::registry::Registry;
use wf_integration::CodeContextService;
use wf_tools::registry::ToolRegistry;

use crate::registry::RegisterOptions;
use crate::result::Summary;

/// All registry-facing tool definitions for the predefined tool set.
pub fn builtin_tools() -> Vec<wf_types::tool::Tool> {
    wf_tools::predefined::builtin_tool_defs()
}

/// Whether one tool is visible under the live service state.
///
/// Tools outside the code-context family are always visible. A code-context
/// tool follows the availability of its call scope; with no service attached
/// at all the service is off, so those tools stay hidden.
pub fn visible(tool_id: &str, service: Option<&CodeContextService>) -> bool {
    match wf_tools::predefined::code_context::scope_of(tool_id) {
        None => true,
        Some(scope) => service.is_some_and(|service| service.availability().covers(scope)),
    }
}

/// Code-context tools visible under the live service state, in declaration
/// order.
pub fn visible_code_context_tools(service: Option<&CodeContextService>) -> Vec<&'static str> {
    wf_tools::predefined::code_context::ALL
        .iter()
        .map(|definition| definition.id)
        .filter(|id| visible(id, service))
        .collect()
}

pub fn register(tool_registry: &ToolRegistry, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for tool_def in builtin_tools() {
        let id = tool_def.id.clone();
        if !visible(&id, opts.code_context.as_deref()) {
            continue;
        }
        if opts.skip_if_exists && tool_registry.has(&id) {
            total.merge(Summary::ok(&id));
            continue;
        }
        tool_registry.register_tool(tool_def);
        total.merge(Summary::ok(&id));
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_integration::{CodeContextConfig, ServiceTransport};

    fn enabled_service() -> CodeContextService {
        CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    #[test]
    fn code_context_tools_follow_the_live_service_state() {
        // No service handle: the feature is off, so the tools stay hidden.
        assert!(!visible("code_search", None));
        assert!(!visible("read_file_folded", None));
        // An enabled service with no verdict yet is not evidence of failure.
        let service = enabled_service();
        assert!(visible("code_search", Some(&service)));
        assert!(visible("read_file_folded", Some(&service)));
        // A disabled service hides every channel.
        let disabled = CodeContextService::disabled();
        assert!(!visible("code_search", Some(&disabled)));
        // Non code-context tools are never filtered.
        assert!(visible("read_file", Some(&disabled)));
        assert!(visible("execute_command", None));
    }

    #[test]
    fn registration_skips_hidden_tools() {
        let registry = ToolRegistry::new();
        let opts = RegisterOptions::default();
        register(&registry, &opts);
        assert!(!registry.has("code_search"));

        let opts = RegisterOptions {
            code_context: Some(std::sync::Arc::new(enabled_service())),
            ..Default::default()
        };
        let registry = ToolRegistry::new();
        register(&registry, &opts);
        assert!(registry.has("code_search"));
        assert!(registry.has("code_symbols"));
        assert!(registry.has("read_file_folded"));
    }
}
