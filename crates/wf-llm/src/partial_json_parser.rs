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
            if let Some(recovered) = recover_partial_json(input) {
                PartialParseResult::Complete(recovered)
            } else if is_potentially_partial_json(input) {
                PartialParseResult::Partial(input.to_string())
            } else {
                PartialParseResult::Invalid
            }
        }
    }
}

/// Recover a parseable value from an incomplete JSON stream (port of the
/// deprecated TS `partialParse`): tokenize the input, strip trailing
/// incomplete tokens, auto-close unclosed containers and re-parse.
///
/// Examples: `{"a": 1,` -> `{"a": 1}`; `{"key": "value"` -> `{"key":"value"}`;
/// `[1, 2` -> `[1, 2]`. Returns `None` when no recovery is possible.
pub fn recover_partial_json(input: &str) -> Option<Value> {
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return None;
    }
    let stripped = strip(&tokens);
    if stripped.is_empty() {
        return None;
    }
    let closed = unstrip(&stripped);
    let generated = generate(&closed);
    serde_json::from_str(&generated).ok()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    String(String),
    Brace(char),
    Paren(char),
    Separator(char),
    Delimiter(char),
    Number(String),
    Name(String),
}

/// Tokenize a (possibly incomplete) JSON string, skipping dangling tokens the
/// same way the TS reference does (unterminated strings, partial literals).
fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '{' | '}' => {
                tokens.push(Token::Brace(ch));
                i += 1;
            }
            '[' | ']' => {
                tokens.push(Token::Paren(ch));
                i += 1;
            }
            ':' => {
                tokens.push(Token::Separator(ch));
                i += 1;
            }
            ',' => {
                tokens.push(Token::Delimiter(ch));
                i += 1;
            }
            '"' => {
                let mut value = String::new();
                let mut dangling_quote = false;
                i += 1;

                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                        if i == chars.len() {
                            dangling_quote = true;
                            break;
                        }
                        let next = chars[i];
                        match next {
                            '"' => value.push('"'),
                            '\\' => value.push('\\'),
                            '/' => value.push('/'),
                            'b' => value.push('\u{0008}'),
                            'f' => value.push('\u{000C}'),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            'u' => {
                                let mut hex = String::new();
                                for _ in 0..4 {
                                    i += 1;
                                    if i == chars.len() {
                                        dangling_quote = true;
                                        break;
                                    }
                                    hex.push(chars[i]);
                                }
                                if !dangling_quote {
                                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                        if let Some(c) = char::from_u32(code) {
                                            value.push(c);
                                        }
                                    }
                                }
                            }
                            other => {
                                value.push('\\');
                                value.push(other);
                            }
                        }
                        i += 1;
                    } else {
                        value.push(chars[i]);
                        i += 1;
                    }
                }

                // Skip past the closing quote (or the dangling end).
                if i < chars.len() {
                    i += 1;
                }

                if !dangling_quote {
                    tokens.push(Token::String(value));
                }
            }
            c if c.is_ascii_whitespace() => {
                i += 1;
            }
            c if c.is_ascii_digit() || c == '-' || c == '.' => {
                let mut value = String::new();
                if c == '-' {
                    value.push(c);
                    i += 1;
                }
                while i < chars.len()
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == 'e'
                        || chars[i] == 'E')
                {
                    value.push(chars[i]);
                    i += 1;
                }
                // Handle e/E followed by +/- sign.
                if i < chars.len()
                    && (chars[i] == '+' || chars[i] == '-')
                    && value.ends_with(['e', 'E'])
                {
                    value.push(chars[i]);
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        value.push(chars[i]);
                        i += 1;
                    }
                }
                tokens.push(Token::Number(value));
            }
            c if c.is_ascii_alphabetic() => {
                let mut value = String::new();
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    value.push(chars[i]);
                    i += 1;
                }
                if matches!(value.as_str(), "true" | "false" | "null") {
                    tokens.push(Token::Name(value));
                }
                // Incomplete literals (e.g. `nul`, `tru`) are skipped.
            }
            _ => {
                i += 1;
            }
        }
    }

    tokens
}

/// Recursively remove trailing tokens that would make the stream unparseable
/// (dangling separators/delimiters, numbers ending in `.`/`-`, key-name strings).
fn strip(tokens: &[Token]) -> Vec<Token> {
    if tokens.is_empty() {
        return tokens.to_vec();
    }

    let mut result = tokens.to_vec();
    let last = result.last().unwrap();

    match last {
        Token::Separator(_) | Token::Delimiter(_) => {
            result.pop();
            return strip(&result);
        }
        Token::Number(value) => {
            if value.ends_with(['.', '-']) {
                result.pop();
                return strip(&result);
            }
        }
        Token::String(_) => {
            let len = result.len();
            let before = if len >= 2 {
                Some(&result[len - 2])
            } else {
                None
            };
            match before {
                Some(Token::Delimiter(_)) | Some(Token::Brace('{')) => {
                    result.pop();
                    return strip(&result);
                }
                _ => {}
            }
        }
        _ => {}
    }

    result
}

/// Auto-close unclosed containers by appending the missing closing symbols.
fn unstrip(tokens: &[Token]) -> Vec<Token> {
    let mut stack: Vec<char> = Vec::new();

    for token in tokens {
        match token {
            Token::Brace('{') => stack.push('}'),
            Token::Brace('}') if stack.last() == Some(&'}') => {
                stack.pop();
            }
            Token::Paren('[') => stack.push(']'),
            Token::Paren(']') if stack.last() == Some(&']') => {
                stack.pop();
            }
            _ => {}
        }
    }

    let mut result = tokens.to_vec();
    for close in stack.iter().rev() {
        if *close == '}' {
            result.push(Token::Brace('}'));
        } else {
            result.push(Token::Paren(']'));
        }
    }
    result
}

/// Serialize the token stream back to a JSON string (strings re-encoded).
fn generate(tokens: &[Token]) -> String {
    let mut output = String::new();
    for token in tokens {
        match token {
            Token::String(value) => {
                output.push_str(&serde_json::to_string(value).unwrap_or_default());
            }
            Token::Brace(c) | Token::Paren(c) | Token::Separator(c) | Token::Delimiter(c) => {
                output.push(*c);
            }
            Token::Number(value) => output.push_str(value),
            Token::Name(value) => output.push_str(value),
        }
    }
    output
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
        match result {
            PartialParseResult::Complete(v) => {
                assert_eq!(v["key"], serde_json::json!("va"));
            }
            other => panic!("expected recovered value, got {other:?}"),
        }
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

    #[test]
    fn recovers_trailing_comma_and_unclosed_object() {
        let recovered = recover_partial_json(r#"{"a": 1,"#).expect("must recover");
        assert_eq!(recovered, serde_json::json!({"a": 1}));
    }

    #[test]
    fn recovers_unclosed_string_value() {
        let recovered = recover_partial_json(r#"{"key": "va"#).expect("must recover");
        assert_eq!(recovered, serde_json::json!({"key": "va"}));
    }

    #[test]
    fn recovers_partial_number_and_nested_arrays() {
        let recovered = recover_partial_json(r#"{"a": [1, 2."#).expect("must recover");
        assert_eq!(recovered, serde_json::json!({"a": [1]}));
    }

    #[test]
    fn recovers_dangling_key() {
        let recovered = recover_partial_json(r#"{"a": 1, "b""#).expect("must recover");
        assert_eq!(recovered, serde_json::json!({"a": 1}));
    }

    #[test]
    fn recovers_partial_literal() {
        // The incomplete literal and its dangling key are dropped
        // (matches the TS reference behavior).
        let recovered = recover_partial_json(r#"{"ok": tru"#).expect("must recover");
        assert_eq!(recovered, serde_json::json!({}));
    }

    #[test]
    fn cannot_recover_garbage() {
        assert!(recover_partial_json("hello world").is_none());
        assert!(recover_partial_json("").is_none());
    }

    #[test]
    fn string_escapes_are_round_tripped() {
        let recovered = recover_partial_json(r#"{"msg": "say \"hi\"\n"#).expect("must recover");
        assert_eq!(recovered["msg"], serde_json::json!("say \"hi\"\n"));
    }

    #[test]
    fn complete_input_recovery_matches_parse() {
        let input = r#"{"a": [1, 2, 3], "b": {"c": true}}"#;
        assert_eq!(
            recover_partial_json(input),
            Some(serde_json::from_str(input).unwrap())
        );
    }
}
