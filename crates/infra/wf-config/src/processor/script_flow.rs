use std::collections::{HashMap, HashSet};

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_script::ScriptFlow;

pub fn validate_script_flow(flow: &ScriptFlow) -> ConfigResult<()> {
    validate_required(&flow.name, "name")?;
    if flow.branches.is_empty() {
        return Err(ConfigError::Validation(format!(
            "flow '{}' needs at least one branch",
            flow.name
        )));
    }
    let mut seen = HashSet::new();
    for branch in &flow.branches {
        validate_required(&branch.key, "branches.key")?;
        if !seen.insert(branch.key.clone()) {
            return Err(ConfigError::Validation(format!(
                "flow '{}' has a duplicate branch '{}'",
                flow.name, branch.key
            )));
        }
        if branch.modules.is_empty() {
            return Err(ConfigError::Validation(format!(
                "flow '{}' branch '{}' needs at least one module",
                flow.name, branch.key
            )));
        }
        for module in &branch.modules {
            validate_required(&module.key, "branches.modules.key")?;
        }
    }
    let keys: HashSet<&str> = flow.branches.iter().map(|b| b.key.as_str()).collect();
    for branch in &flow.branches {
        if let Some(deps) = &branch.depends_on {
            for dep in deps {
                if !keys.contains(dep.as_str()) {
                    return Err(ConfigError::Validation(format!(
                        "flow '{}' branch '{}' depends on unknown branch '{}'",
                        flow.name, branch.key, dep
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn transform_script_flow(
    flow: &ScriptFlow,
    parameters: &HashMap<String, String>,
) -> ConfigResult<ScriptFlow> {
    let mut cloned = flow.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_script_flow(flow: ScriptFlow) -> ScriptFlow {
    flow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_flow() -> ScriptFlow {
        ScriptFlow {
            name: "ci".to_string(),
            branches: vec![wf_script::FlowBranch {
                key: "build".to_string(),
                depends_on: None,
                modules: vec![wf_script::ModuleRef {
                    key: "compile".to_string(),
                    args: None,
                }],
            }],
        }
    }

    #[test]
    fn test_valid_flow() {
        let flow = make_flow();
        assert!(validate_script_flow(&flow).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut flow = make_flow();
        flow.name = String::new();
        assert!(validate_script_flow(&flow).is_err());
    }

    #[test]
    fn test_unknown_dependency() {
        let mut flow = make_flow();
        flow.branches[0].depends_on = Some(vec!["missing".to_string()]);
        assert!(validate_script_flow(&flow).is_err());
    }

    #[test]
    fn test_transform_script_flow() {
        let flow = make_flow();
        let mut params = HashMap::new();
        params.insert("branch".to_string(), "main".to_string());

        let result = transform_script_flow(&flow, &params).unwrap();
        assert_eq!(result.name, "ci");
    }

    #[test]
    fn test_export_script_flow() {
        let flow = make_flow();
        let exported = export_script_flow(flow.clone());
        assert_eq!(exported.name, flow.name);
    }
}
