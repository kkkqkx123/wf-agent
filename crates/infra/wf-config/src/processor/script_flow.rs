use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::script::flow::ScriptFlow;

pub fn validate_script_flow(flow: &ScriptFlow) -> ConfigResult<()> {
    validate_required(&flow.script_id, "script_id")?;
    validate_required(&flow.flow_type, "flow_type")?;
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
            script_id: "flow-1".to_string(),
            flow_type: "conditional".to_string(),
            config: None,
        }
    }

    #[test]
    fn test_valid_flow() {
        let flow = make_flow();
        assert!(validate_script_flow(&flow).is_ok());
    }

    #[test]
    fn test_empty_script_id() {
        let mut flow = make_flow();
        flow.script_id = String::new();
        assert!(validate_script_flow(&flow).is_err());
    }

    #[test]
    fn test_transform_script_flow() {
        let flow = make_flow();
        let mut params = HashMap::new();
        params.insert("branch".to_string(), "main".to_string());

        let result = transform_script_flow(&flow, &params).unwrap();
        assert_eq!(result.script_id, "flow-1");
    }

    #[test]
    fn test_export_script_flow() {
        let flow = make_flow();
        let exported = export_script_flow(flow.clone());
        assert_eq!(exported.script_id, flow.script_id);
    }
}
