use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;

use crate::error::ToolResult;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct BaseExecutor;

impl BaseExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_parameters(
        tool: &wf_types::tool::Tool,
        parameters: &Value,
    ) -> ToolResult<()> {
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

    fn validate_property(
        key: &str,
        value: &Value,
        prop: &wf_types::tool::ToolProperty,
    ) -> ToolResult<()> {
        if let Some(ref expected_type) = prop.r#type {
            Self::validate_type(key, value, expected_type)?;
        }

        if let Some(ref enum_values) = prop.value.as_array() {
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

    fn validate_string(key: &str, s: &str, prop: &wf_types::tool::ToolProperty) -> ToolResult<()> {
        if let Some(min_len) = prop.value.get("minLength").and_then(|v| v.as_u64()) {
            if s.len() < min_len as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} is below minimum {}",
                    key,
                    s.len(),
                    min_len
                )));
            }
        }
        if let Some(max_len) = prop.value.get("maxLength").and_then(|v| v.as_u64()) {
            if s.len() > max_len as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} exceeds maximum {}",
                    key,
                    s.len(),
                    max_len
                )));
            }
        }
        if let Some(pattern) = prop.value.get("pattern").and_then(|v| v.as_str()) {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(s) {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' does not match pattern '{}'",
                        key, pattern
                    )));
                }
            }
        }
        if let Some(format) = prop.value.get("format").and_then(|v| v.as_str()) {
            Self::validate_format(key, s, format)?;
        }
        Ok(())
    }

    fn validate_format(key: &str, s: &str, format: &str) -> ToolResult<()> {
        match format {
            "date-time" => {
                let dt_re = Regex::new(
                    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$"
                ).unwrap();
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
                if !s.starts_with("http://") && !s.starts_with("https://") && !s.starts_with("file://") {
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
                if octets.len() != 4
                    || octets.iter().any(|o| o.parse::<u8>().is_err())
                {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid IPv4 address: {}",
                        key, s
                    )));
                }
            }
            "ipv6"
                if (!s.contains(':') || s.split(':').count() > 8) => {
                    return Err(crate::error::ToolError::ValidationFailed(format!(
                        "Parameter '{}' is not a valid IPv6 address: {}",
                        key, s
                    )));
                }
            _ => {}
        }
        Ok(())
    }

    fn validate_number(key: &str, n: &serde_json::Number, prop: &wf_types::tool::ToolProperty) -> ToolResult<()> {
        let num = n.as_f64().unwrap_or(f64::NAN);
        if let Some(min) = prop.value.get("minimum").and_then(|v| v.as_f64()) {
            if num < min {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is below minimum {}",
                    key, num, min
                )));
            }
        }
        if let Some(ex_min) = prop.value.get("exclusiveMinimum").and_then(|v| v.as_f64()) {
            if num <= ex_min {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is not greater than exclusiveMinimum {}",
                    key, num, ex_min
                )));
            }
        }
        if let Some(max) = prop.value.get("maximum").and_then(|v| v.as_f64()) {
            if num > max {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} exceeds maximum {}",
                    key, num, max
                )));
            }
        }
        if let Some(ex_max) = prop.value.get("exclusiveMaximum").and_then(|v| v.as_f64()) {
            if num >= ex_max {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' value {} is not less than exclusiveMaximum {}",
                    key, num, ex_max
                )));
            }
        }
        Ok(())
    }

    fn validate_array(key: &str, arr: &[Value], prop: &wf_types::tool::ToolProperty) -> ToolResult<()> {
        if let Some(min_items) = prop.value.get("minItems").and_then(|v| v.as_u64()) {
            if arr.len() < min_items as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} is below minItems {}",
                    key,
                    arr.len(),
                    min_items
                )));
            }
        }
        if let Some(max_items) = prop.value.get("maxItems").and_then(|v| v.as_u64()) {
            if arr.len() > max_items as usize {
                return Err(crate::error::ToolError::ValidationFailed(format!(
                    "Parameter '{}' length {} exceeds maxItems {}",
                    key,
                    arr.len(),
                    max_items
                )));
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
