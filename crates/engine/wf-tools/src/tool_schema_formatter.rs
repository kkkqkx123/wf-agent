use wf_types::tool::schema::{ToolParametersSchema, ToolPropertySchema};

pub struct ToolSchemaFormatter;

impl ToolSchemaFormatter {
    pub fn format_property(_name: &str, prop: &ToolPropertySchema) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        if let Some(ref r) = prop.r#ref {
            map.insert("$ref".to_string(), serde_json::json!(r));
        }

        map.insert("type".to_string(), serde_json::json!(prop.property_type));

        if let Some(ref desc) = prop.description {
            map.insert("description".to_string(), serde_json::json!(desc));
        }

        if let Some(ref enum_vals) = prop.r#enum {
            map.insert("enum".to_string(), serde_json::json!(enum_vals));
        }

        if let Some(ref items) = prop.items {
            map.insert("items".to_string(), Self::format_property(_name, items));
        }

        if let Some(ref properties) = prop.properties {
            let mut props_map = serde_json::Map::new();
            for (k, v) in properties {
                props_map.insert(k.clone(), Self::format_property(k, v));
            }
            map.insert(
                "properties".to_string(),
                serde_json::Value::Object(props_map),
            );
        }

        if let Some(ref required) = prop.required {
            map.insert("required".to_string(), serde_json::json!(required));
        }

        if let Some(ref additional) = prop.additional_properties {
            map.insert(
                "additionalProperties".to_string(),
                serde_json::json!(additional),
            );
        }

        if let Some(ref default) = prop.default {
            map.insert("default".to_string(), serde_json::json!(default));
        }

        if let Some(ref pattern) = prop.pattern {
            map.insert("pattern".to_string(), serde_json::json!(pattern));
        }

        if let Some(ref v) = prop.min_length {
            map.insert("minLength".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.max_length {
            map.insert("maxLength".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.minimum {
            map.insert("minimum".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.maximum {
            map.insert("maximum".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.exclusive_minimum {
            map.insert("exclusiveMinimum".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.exclusive_maximum {
            map.insert("exclusiveMaximum".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.min_items {
            map.insert("minItems".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.max_items {
            map.insert("maxItems".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.min_properties {
            map.insert("minProperties".to_string(), serde_json::json!(v));
        }
        if let Some(ref v) = prop.format {
            map.insert("format".to_string(), serde_json::json!(v));
        }

        serde_json::Value::Object(map)
    }

    pub fn format_parameters(params: &ToolParametersSchema) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "type".to_string(),
            serde_json::json!(params.parameters_type),
        );

        let mut props_map = serde_json::Map::new();
        for (k, v) in &params.properties {
            props_map.insert(k.clone(), Self::format_property(k, v));
        }
        map.insert(
            "properties".to_string(),
            serde_json::Value::Object(props_map),
        );

        if let Some(ref required) = params.required {
            map.insert("required".to_string(), serde_json::json!(required));
        }

        serde_json::Value::Object(map)
    }

    pub fn clean_schema(schema: &serde_json::Value) -> serde_json::Value {
        match schema {
            serde_json::Value::Object(obj) => {
                let mut cleaned = serde_json::Map::new();
                for (k, v) in obj {
                    if k.starts_with("$") || k == "additionalProperties" {
                        continue;
                    }
                    cleaned.insert(k.clone(), Self::clean_schema(v));
                }
                serde_json::Value::Object(cleaned)
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Self::clean_schema).collect())
            }
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_prop() -> ToolPropertySchema {
        ToolPropertySchema {
            r#ref: None,
            property_type: "string".to_string(),
            description: Some("A test property".to_string()),
            r#enum: None,
            items: None,
            properties: None,
            required: None,
            additional_properties: None,
            default: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            exclusive_minimum: None,
            exclusive_maximum: None,
            min_items: None,
            max_items: None,
            min_properties: None,
            format: None,
        }
    }

    #[test]
    fn test_format_property() {
        let formatted = ToolSchemaFormatter::format_property("test", &string_prop());
        assert_eq!(formatted["type"], "string");
        assert_eq!(formatted["description"], "A test property");
    }

    #[test]
    fn test_format_property_with_constraints() {
        let mut prop = string_prop();
        prop.r#enum = Some(vec![
            serde_json::json!("text"),
            serde_json::json!("markdown"),
        ]);
        prop.default = Some(serde_json::json!("markdown"));
        prop.pattern = Some("^[a-z]+$".to_string());
        prop.min_length = Some(1);
        prop.max_length = Some(64);

        let formatted = ToolSchemaFormatter::format_property("test", &prop);
        assert_eq!(formatted["enum"], serde_json::json!(["text", "markdown"]));
        assert_eq!(formatted["default"], serde_json::json!("markdown"));
        assert_eq!(formatted["pattern"], "^[a-z]+$");
        assert_eq!(formatted["minLength"], 1);
        assert_eq!(formatted["maxLength"], 64);
    }

    #[test]
    fn test_clean_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string"}
            }
        });

        let cleaned = ToolSchemaFormatter::clean_schema(&schema);
        assert!(cleaned.get("$schema").is_none());
        assert!(cleaned.get("additionalProperties").is_none());
        assert!(cleaned.get("type").is_some());
    }
}
