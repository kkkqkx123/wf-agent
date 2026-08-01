use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum PartialParseResult {
    Complete(Value),
    Partial(String),
    Invalid,
}

impl PartialParseResult {
    pub fn as_complete(&self) -> Option<&Value> {
        match self {
            PartialParseResult::Complete(v) => Some(v),
            _ => None,
        }
    }
}

pub fn parse_partial_json(input: &str) -> PartialParseResult {
    match serde_json::from_str::<Value>(input) {
        Ok(v) => PartialParseResult::Complete(v),
        Err(_) => {
            if is_potentially_partial_json(input) {
                PartialParseResult::Partial(input.to_string())
            } else {
                PartialParseResult::Invalid
            }
        }
    }
}

fn is_potentially_partial_json(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut has_open_bracket = false;

    for ch in trimmed.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }

        match ch {
            '{' | '[' => {
                depth += 1;
                has_open_bracket = true;
            }
            '}' | ']' => depth -= 1,
            _ => {}
        }
    }

    has_open_bracket && depth > 0
}

pub fn accumulate_and_parse(chunks: &[&str]) -> PartialParseResult {
    let combined = chunks.concat();
    parse_partial_json(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_json() {
        let result = parse_partial_json(r#"{"key": "value"}"#);
        assert!(matches!(result, PartialParseResult::Complete(_)));
    }

    #[test]
    fn test_partial_json() {
        let result = parse_partial_json(r#"{"key": "va"#);
        assert_eq!(
            result,
            PartialParseResult::Partial(r#"{"key": "va"#.to_string())
        );
    }

    #[test]
    fn test_invalid_json() {
        let result = parse_partial_json("not json at all");
        assert_eq!(result, PartialParseResult::Invalid);
    }

    #[test]
    fn test_accumulate() {
        let chunks = vec![r#"{"key": ""#, r#"value"}"#];
        let result = accumulate_and_parse(&chunks);
        assert!(matches!(result, PartialParseResult::Complete(_)));
    }
}
