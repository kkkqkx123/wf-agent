pub const MAX_PAYLOAD_CHARS: usize = 4000;

/// Truncate text to MAX_PAYLOAD_CHARS, appending a truncation marker when cut.
pub fn cap_payload_text(text: &str) -> String {
    if text.len() <= MAX_PAYLOAD_CHARS {
        return text.to_string();
    }
    let mut end = MAX_PAYLOAD_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated {} chars]", &text[..end], text.len() - end)
}

/// Truncate a JSON value by serializing to string, capping, and wrapping
/// back into a JSON string value when truncation occurs.
pub fn cap_json_value(value: &serde_json::Value) -> serde_json::Value {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.len() <= MAX_PAYLOAD_CHARS {
        return value.clone();
    }
    serde_json::Value::String(cap_payload_text(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_cap_truncates_long_text() {
        let long = "x".repeat(MAX_PAYLOAD_CHARS + 10);
        let capped = cap_payload_text(&long);
        assert!(capped.contains("truncated"));
    }

    #[test]
    fn payload_cap_short_text_unchanged() {
        let short = "hello";
        assert_eq!(cap_payload_text(short), "hello");
    }

    #[test]
    fn cap_json_value_short_passthrough() {
        let value = serde_json::json!({"key": "val"});
        assert_eq!(cap_json_value(&value), value);
    }

    #[test]
    fn cap_json_value_long_becomes_string() {
        let value = serde_json::json!({"key": "x".repeat(MAX_PAYLOAD_CHARS + 10)});
        let capped = cap_json_value(&value);
        assert!(capped.is_string());
        assert!(capped.as_str().unwrap().contains("truncated"));
    }
}
