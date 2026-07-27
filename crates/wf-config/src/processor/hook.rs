use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::workflow::hook_template::HookTemplate;

pub fn validate_hook_template(template: &HookTemplate) -> ConfigResult<()> {
    validate_required(&template.id, "id")?;
    validate_required(&template.name, "name")?;
    Ok(())
}

pub fn transform_hook_template(
    template: &HookTemplate,
    parameters: &HashMap<String, String>,
) -> ConfigResult<HookTemplate> {
    let mut cloned = template.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_hook_template(template: HookTemplate) -> HookTemplate {
    template
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_template() -> HookTemplate {
        HookTemplate {
            id: "hook-1".to_string(),
            name: "before-node".to_string(),
            description: "Runs before node".to_string(),
            hook_type: wf_types::workflow::hook_template::WorkflowHookType::BeforeNode,
            default_config: None,
        }
    }

    #[test]
    fn test_valid_template() {
        let template = make_template();
        assert!(validate_hook_template(&template).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut template = make_template();
        template.name = String::new();
        assert!(validate_hook_template(&template).is_err());
    }

    #[test]
    fn test_transform_hook_template() {
        let template = make_template();
        let mut params = HashMap::new();
        params.insert("stage".to_string(), "pre".to_string());

        let result = transform_hook_template(&template, &params).unwrap();
        assert_eq!(result.name, "before-node");
    }

    #[test]
    fn test_export_hook_template() {
        let template = make_template();
        let exported = export_hook_template(template.clone());
        assert_eq!(exported.id, template.id);
        assert_eq!(exported.name, template.name);
    }
}
