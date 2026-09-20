pub mod prompts;
pub mod tools;
pub mod triggers;

pub use prompts::register_custom_prompts;
pub use tools::register_custom_tools;
pub use triggers::register_custom_triggers;

use wf_tools::registry::ToolRegistry;

use crate::custom::types::{CustomResources, CustomValidationLevel};
use crate::registry::ResourceRegistries;
use crate::result::Summary;

pub fn register_custom_resources(
    regs: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    resources: CustomResources,
    skip_if_exists: bool,
    validation_level: CustomValidationLevel,
) -> Summary {
    let mut total = Summary::new();

    if !resources.errors.is_empty() {
        if validation_level == CustomValidationLevel::Strict {
            // Strict mode: any load/parse failure aborts the whole custom
            // resource pipeline; nothing is registered partially.
            for err in &resources.errors {
                total.merge(Summary::err("custom_load.strict", err));
            }
            return total;
        }
        for err in &resources.errors {
            total.merge(Summary::err("custom_load", err));
        }
    }

    let r = register_custom_tools(tool_registry, resources.tools, skip_if_exists);
    total.merge(r);

    let r = register_custom_triggers(&regs.trigger_templates, resources.triggers, skip_if_exists);
    total.merge(r);

    let r = register_custom_prompts(regs, resources.prompts, skip_if_exists);
    total.merge(r);

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::types::{
        CustomHandlerConfig, CustomParamSchema, CustomToolDefinition, CustomToolType,
    };
    use wf_core::registry::Registry;

    fn make_resources_with_error() -> CustomResources {
        CustomResources {
            tools: vec![CustomToolDefinition {
                id: "custom-tool".into(),
                tool_type: CustomToolType::Stateless,
                description: "A custom tool".into(),
                schema: CustomParamSchema { parameters: vec![] },
                handler: CustomHandlerConfig::Inline { code: "x".into() },
                metadata: None,
            }],
            triggers: vec![],
            prompts: vec![],
            errors: vec!["cannot read tools.json: parse error".into()],
        }
    }

    #[test]
    fn test_lenient_registers_partial() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let summary = register_custom_resources(
            &regs,
            &tool_registry,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Lenient,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load"));
        assert!(tool_registry.has("custom-tool"));
    }

    #[test]
    fn test_strict_aborts_pipeline() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let summary = register_custom_resources(
            &regs,
            &tool_registry,
            make_resources_with_error(),
            false,
            CustomValidationLevel::Strict,
        );
        assert!(summary.failed.iter().any(|f| f.id == "custom_load.strict"));
        assert!(!tool_registry.has("custom-tool"));
    }
}
