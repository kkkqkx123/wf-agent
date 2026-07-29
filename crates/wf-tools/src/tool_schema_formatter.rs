use wf_types::tool::schema::{ToolParametersSchema, ToolPropertySchema};

pub struct ToolSchemaFormatter;

impl ToolSchemaFormatter {
    pub fn format_property(_name: &str, prop: &ToolPropertySchema) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        map.insert("type".to_string(), serde_json::json!(prop.property_type));

        if let Some(ref desc) = prop.description {
            map.insert("description".to_string(), serde_json::json!(desc));
        }

        if let Some(ref items) = prop.items {
            map.insert("items".to_string(), Self::format_property(_name, items));
        }

        if let Some(ref properties) = prop.properties {
            let mut props_map = serde_json::Map::new();
            for (k, v) in properties {
                props_map.insert(k.clone(), Self::format_property(k, v));
            }
            map.insert("properties".to_string(), serde_json::Value::Object(props_map));
        }

        if let Some(ref required) = prop.required {
            map.insert("required".to_string(), serde_json::json!(required));
        }

        if let Some(ref enum_vals) = prop.r#enum {
            map.insert("enum".to_string(), serde_json::json!(enum_vals));
        }

        serde_json::Value::Object(map)
    }

    pub fn format_parameters(params: &ToolParametersSchema) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), serde_json::json!(params.parameters_type));

        let mut props_map = serde_json::Map::new();
        for (k, v) in &params.properties {
            props_map.insert(k.clone(), Self::format_property(k, v));
        }
        map.insert("properties".to_string(), serde_json::Value::Object(props_map));

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

    #[test]
    fn test_format_property() {
        let prop = ToolPropertySchema {
            property_type: "string".to_string(),
            description: Some("A test property".to_string()),
            items: None,
            properties: None,
            required: None,
            r#enum: None,
            default: None,
        };

        let formatted = ToolSchemaFormatter::format_property("test", &prop);
        assert_eq!(formatted["type"], "string");
        assert_eq!(formatted["description"], "A test property");
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
