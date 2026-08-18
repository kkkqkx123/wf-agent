use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;

use crate::error::ToolResult;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;
use wf_types::tool::ToolPropertySchema;

pub struct BaseExecutor;

impl BaseExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_parameters(tool: &wf_types::tool::Tool, parameters: &Value) -> ToolResult<()> {
        let Some(schema) = &tool.parameters else {
            return Ok(());
        };

        let Value::Object(params) = parameters else {
            return Err(crate::error::ToolError::ValidationFailed(
                "Parameters must be a JSON object".into(),
            ));
        };

        for required_field in &schema.required {
            if !params.contains_key(required_field) {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Missing required parameter: {}",
                    required_field
                )));
            }
        }

        if schema.additional_properties == Some(false) {
            let allowed: HashSet<&str> = schema.properties.keys().map(|s| s.as_str()).collect();
            for key in params.keys() {
                if !allowed.contains(key.as_str()) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Unknown parameter: {}",
                        key
                    )));
                }
            }
        }

        for (key, value) in params {
            if let Some(prop) = schema.properties.get(key) {
                Self::validate_property(key, value, prop)?;
            }
        }

        Ok(())
    }

    fn validate_property(key: &str, value: &Value, prop: &ToolPropertySchema) -> ToolResult<()> {
        Self::validate_type(key, value, &prop.property_type)?;

        if let Some(ref enum_values) = prop.r#enum {
            if !enum_values.is_empty() && !enum_values.contains(value) {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' must be one of: {:?}",
                    key, enum_values
                )));
            }
        }

        match value {
            Value::String(s) => Self::validate_string(key, s, prop),
            Value::Number(n) => Self::validate_number(key, n, prop),
            Value::Array(arr) => Self::validate_array(key, arr, prop),
            Value::Object(obj) => Self::validate_object(key, obj, prop),
            _ => Ok(()),
        }
    }

    fn validate_type(key: &str, value: &Value, expected: &str) -> ToolResult<()> {
        let type_matches = match expected {
            "string" => matches!(value, Value::String(_)),
            "number" => matches!(value, Value::Number(_)),
            "integer" => matches!(value, Value::Number(n) if n.is_i64() || n.is_u64()),
            "boolean" => matches!(value, Value::Bool(_)),
            "array" => matches!(value, Value::Array(_)),
            "object" => matches!(value, Value::Object(_)),
            "null" => matches!(value, Value::Null),
            _ => false,
        };

        if !type_matches {
            return Err(crate::error::ToolError::ValidationFailed(format!(
                "Parameter '{}' expected type '{}' but got '{}'",
                key,
                expected,
                Self::json_type_name(value)
            )));
        }
        Ok(())
    }

    fn validate_string(key: &str, s: &str, prop: &ToolPropertySchema) -> ToolResult<()> {
        if let Some(min_len) = prop.min_length {
            if s.len() < min_len as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} is below minimum {}",
                    key,
                    s.len(),
                    min_len
                )));
            }
        }
        if let Some(max_len) = prop.max_length {
            if s.len() > max_len as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} exceeds maximum {}",
                    key,
                    s.len(),
                    max_len
                )));
            }
        }
        if let Some(pattern) = &prop.pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' does not match pattern '{}'",
                        key, pattern
                    )));
                }
            }
        }
        if let Some(format) = &prop.format {
            Self::validate_format(key, s, format)?;
        }
        Ok(())
    }

    fn validate_format(key: &str, s: &str, format: &str) -> ToolResult<()> {
        match format {
            "date-time" => {
                let dt_re =
                    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$")
                        .unwrap();
                if !dt_re.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid date-time (RFC3339): {}",
                        key, s
                    )));
                }
            }
            "date" => {
                let d_re = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
                if !d_re.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid date (YYYY-MM-DD): {}",
                        key, s
                    )));
                }
            }
            "time" => {
                let t_re = Regex::new(r"^\d{2}:\d{2}:\d{2}(\.\d+)?$").unwrap();
                if !t_re.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid time (HH:MM:SS): {}",
                        key, s
                    )));
                }
            }
            "uri" => {
                if !s.starts_with("http://")
                    && !s.starts_with("https://")
                    && !s.starts_with("file://")
                {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid URI: {}",
                        key, s
                    )));
                }
            }
            "email" => {
                let has_at = s.contains('@');
                let has_dot = s.contains('.');
                if !has_at || !has_dot || s.starts_with('@') || s.ends_with('.') {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid email: {}",
                        key, s
                    )));
                }
            }
            "uuid" => {
                let uuid_pattern = Regex::new(
                    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
                ).unwrap();
                if !uuid_pattern.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid UUID: {}",
                        key, s
                    )));
                }
            }
            "ipv4" => {
                let octets: Vec<&str> = s.split('.').collect();
                if octets.len() != 4 || octets.iter().any(|o| o.parse::<u8>().is_err()) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid IPv4 address: {}",
                        key, s
                    )));
                }
            }
            "ipv6" if (!s.contains(':') || s.split(':').count() > 8) => {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' is not a valid IPv6 address: {}",
                    key, s
                )));
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_number(
        key: &str,
        n: &serde_json::Number,
        prop: &ToolPropertySchema,
    ) -> ToolResult<()> {
        let num = n.as_f64().unwrap_or(f64::NAN);
        if let Some(min) = prop.minimum {
            if num < min {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is below minimum {}",
                    key, num, min
                )));
            }
        }
        if let Some(ex_min) = prop.exclusive_minimum {
            if num <= ex_min {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is not greater than exclusiveMinimum {}",
                    key, num, ex_min
                )));
            }
        }
        if let Some(max) = prop.maximum {
            if num > max {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} exceeds maximum {}",
                    key, num, max
                )));
            }
        }
        if let Some(ex_max) = prop.exclusive_maximum {
            if num >= ex_max {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is not less than exclusiveMaximum {}",
                    key, num, ex_max
                )));
            }
        }
        Ok(())
    }

    fn validate_array(key: &str, arr: &[Value], prop: &ToolPropertySchema) -> ToolResult<()> {
        if let Some(min_items) = prop.min_items {
            if arr.len() < min_items as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} is below minItems {}",
                    key,
                    arr.len(),
                    min_items
                )));
            }
        }
        if let Some(max_items) = prop.max_items {
            if arr.len() > max_items as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} exceeds maxItems {}",
                    key,
                    arr.len(),
                    max_items
                )));
            }
        }
        if let Some(ref items) = prop.items {
            for (i, item) in arr.iter().enumerate() {
                Self::validate_property(&format!("{}[{}]", key, i), item, items)?;
            }
        }
        Ok(())
    }

    fn validate_object(
        key: &str,
        obj: &serde_json::Map<String, Value>,
        prop: &ToolPropertySchema,
    ) -> ToolResult<()> {
        if let Some(min_properties) = prop.min_properties {
            if obj.len() < min_properties as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' has {} properties, below minProperties {}",
                    key,
                    obj.len(),
                    min_properties
                )));
            }
        }

        let Some(ref properties) = prop.properties else {
            return Ok(());
        };

        if prop.additional_properties == Some(false) {
            let allowed: HashSet<&str> = properties.keys().map(|s| s.as_str()).collect();
            for field in obj.keys() {
                if !allowed.contains(field.as_str()) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Unknown parameter in '{}': {}",
                        key, field
                    )));
                }
            }
        }

        if let Some(ref required) = prop.required {
            for field in required {
                if !obj.contains_key(field) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Missing required field '{}' in parameter '{}'",
                        field, key
                    )));
                }
            }
        }

        for (field, value) in obj {
            if let Some(field_schema) = properties.get(field) {
                Self::validate_property(&format!("{}.{}", key, field), value, field_schema)?;
            }
        }
        Ok(())
    }

    fn json_type_name(value: &Value) -> &'static str {
        match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Null => "null",
        }
    }

    pub fn build_result(
        success: bool,
        result: Option<Value>,
        error: Option<String>,
        execution_time_ms: i64,
        retry_count: u32,
    ) -> ToolExecutionResult {
        ToolExecutionResult {
            success,
            result,
            error,
            execution_time: execution_time_ms,
            retry_count,
        }
    }
}

impl Default for BaseExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for BaseExecutor {
    async fn execute(
        &self,
        _tool: &wf_types::tool::Tool,
        _parameters: &Value,
        _options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        Err(crate::error::ToolError::Internal(
            "BaseExecutor is abstract — use a concrete executor".into(),
        ))
    }

    fn executor_type(&self) -> &str {
        "base"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wf_types::tool::{Tool, ToolParameterSchema, ToolPropertySchema};

    fn property(property_type: &str) -> ToolPropertySchema {
        ToolPropertySchema::typed(property_type)
    }

    fn make_tool(properties: BTreeMap<String, ToolPropertySchema>) -> Tool {
        Tool {
            id: "test".into(),
            name: "test".into(),
            description: "test tool".into(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: Some(ToolParameterSchema {
                r#type: "object".into(),
                properties,
                required: Vec::new(),
                additional_properties: Some(false),
            }),
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[test]
    fn validate_enum_hit_and_miss() {
        let mut format = property("string");
        format.r#enum = Some(vec![
            serde_json::json!("text"),
            serde_json::json!("markdown"),
        ]);
        let tool = make_tool(BTreeMap::from([("format".to_string(), format)]));

        BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "format": "markdown" }))
            .unwrap();
        let err = BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "format": "pdf" }))
            .unwrap_err();
        assert!(err.to_string().contains("must be one of"));
    }

    #[test]
    fn validate_items_recursively() {
        let item = property("string");
        let mut options = property("array");
        options.items = Some(Box::new(item));
        options.min_items = Some(1);
        let tool = make_tool(BTreeMap::from([("options".to_string(), options)]));

        BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "options": ["a", "b"] }))
            .unwrap();

        let err =
            BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "options": [1, "a"] }))
                .unwrap_err();
        assert!(err.to_string().contains("options[0]"), "{}", err);

        let err = BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "options": [] }))
            .unwrap_err();
        assert!(err.to_string().contains("minItems"));
    }

    #[test]
    fn validate_pattern_and_bounds() {
        let mut name = property("string");
        name.pattern = Some("^[a-z]+$".into());
        name.min_length = Some(2);
        name.max_length = Some(8);
        let mut count = property("integer");
        count.minimum = Some(1.0);
        count.maximum = Some(10.0);
        count.exclusive_maximum = Some(10.0);
        let tool = make_tool(BTreeMap::from([
            ("name".to_string(), name),
            ("count".to_string(), count),
        ]));

        BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "name": "abc", "count": 9 }))
            .unwrap();

        assert!(
            BaseExecutor::validate_parameters(
                &tool,
                &serde_json::json!({ "name": "ABC", "count": 9 }),
            )
            .is_err(),
            "pattern must reject 'ABC'"
        );
        assert!(
            BaseExecutor::validate_parameters(
                &tool,
                &serde_json::json!({ "name": "a", "count": 9 }),
            )
            .is_err(),
            "minLength must reject 'a'"
        );
        assert!(
            BaseExecutor::validate_parameters(
                &tool,
                &serde_json::json!({ "name": "abc", "count": 10 }),
            )
            .is_err(),
            "exclusiveMaximum must reject 10"
        );
    }

    #[test]
    fn validate_nested_object_properties() {
        let mut filter = property("object");
        let mut inner = BTreeMap::new();
        inner.insert("limit".to_string(), property("integer"));
        filter.properties = Some(inner);
        filter.required = Some(vec!["limit".into()]);
        let tool = make_tool(BTreeMap::from([("filter".to_string(), filter)]));

        BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "filter": { "limit": 5 } }))
            .unwrap();

        let err = BaseExecutor::validate_parameters(&tool, &serde_json::json!({ "filter": {} }))
            .unwrap_err();
        assert!(
            err.to_string().contains("required field 'limit'"),
            "{}",
            err
        );
    }

    #[test]
    fn validate_unknown_parameter_rejected() {
        let tool = make_tool(BTreeMap::from([("known".to_string(), property("string"))]));
        let err = BaseExecutor::validate_parameters(
            &tool,
            &serde_json::json!({ "known": "x", "sneaky": 1 }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Unknown parameter"));
    }

    #[test]
    fn validate_web_fetch_format_against_predefined_schema() {
        let tool = crate::predefined::web::WEB_FETCH.tool_def();
        BaseExecutor::validate_parameters(
            &tool,
            &serde_json::json!({ "url": "https://example.com", "format": "markdown" }),
        )
        .unwrap();
        let err = BaseExecutor::validate_parameters(
            &tool,
            &serde_json::json!({ "url": "https://example.com", "format": "pdf" }),
        )
        .unwrap_err();
        assert!(err.to_string().contains("must be one of"));
    }
}
