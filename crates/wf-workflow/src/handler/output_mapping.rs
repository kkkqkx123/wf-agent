use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::node::configs::script::ScriptOutputMapping;

pub fn apply_output_mappings(ctx: &NodeExecutionContext, output: &Value, mapping_val: &Value) {
    if let Ok(mappings) = serde_json::from_value::<Vec<ScriptOutputMapping>>(mapping_val.clone()) {
        for mapping in &mappings {
            let value = if let Some(ref path) = mapping.path {
                extract_path_value(output, Some(path))
            } else {
                output.clone()
            };
            match mapping.target.as_str() {
                "variable" => {
                    ctx.set_variable(mapping.key.clone(), value);
                }
                "output" => {
                    ctx.set_variable(mapping.key.clone(), value);
                }
                _ => {}
            }
        }
    } else if let Some(arr) = mapping_val.as_array() {
        for entry in arr {
            let target = entry.get("target").and_then(|v| v.as_str());
            let key = entry.get("key").and_then(|v| v.as_str());
            let path = entry.get("path").and_then(|v| v.as_str());
            let value = extract_path_value(output, path);
            match target {
                Some("variable") => {
                    if let Some(k) = key {
                        ctx.set_variable(k.to_string(), value);
                    }
                }
                Some("output") => {
                    if let Some(k) = key {
                        ctx.set_variable(k.to_string(), value);
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn extract_path_value(value: &Value, path: Option<&str>) -> Value {
    let path = match path {
        Some(p) => p,
        None => return value.clone(),
    };
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for part in parts {
        match current {
            Value::Object(map) => {
                current = map.get(part).unwrap_or(&Value::Null);
            }
            Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    current = arr.get(idx).unwrap_or(&Value::Null);
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

pub fn deep_clone_mapped(value: &Value, mappings: &[ScriptOutputMapping]) -> Value {
    let mut result = value.clone();
    for mapping in mappings {
        let src_value = if let Some(ref path) = mapping.path {
            extract_path_value(value, Some(path))
        } else {
            value.clone()
        };
        if let Value::Object(ref mut map) = result {
            map.insert(mapping.key.clone(), src_value);
        }
    }
    result
}
