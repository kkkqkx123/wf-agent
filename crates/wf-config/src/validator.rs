use std::sync::LazyLock;

use crate::error::{ConfigError, ConfigResult};

static EMAIL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap());

pub fn validate_required(value: &str, field_name: &str) -> ConfigResult<()> {
    if value.is_empty() {
        Err(ConfigError::Validation(format!(
            "{field_name} is required and cannot be empty"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_range<T: PartialOrd + std::fmt::Display>(
    value: T,
    min: T,
    max: T,
    field_name: &str,
) -> ConfigResult<()> {
    if value < min || value > max {
        Err(ConfigError::Validation(format!(
            "{field_name} must be between {min} and {max}, got {value}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_min<T: PartialOrd + std::fmt::Display>(
    value: T,
    min: T,
    field_name: &str,
) -> ConfigResult<()> {
    if value < min {
        Err(ConfigError::Validation(format!(
            "{field_name} must be at least {min}, got {value}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_max<T: PartialOrd + std::fmt::Display>(
    value: T,
    max: T,
    field_name: &str,
) -> ConfigResult<()> {
    if value > max {
        Err(ConfigError::Validation(format!(
            "{field_name} must be at most {max}, got {value}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_enum<T: AsRef<str> + std::fmt::Display>(
    value: T,
    allowed: &[&str],
    field_name: &str,
) -> ConfigResult<()> {
    let v = value.as_ref();
    if !allowed.contains(&v) {
        Err(ConfigError::Validation(format!(
            "{field_name} must be one of [{}], got {value}",
            allowed.join(", ")
        )))
    } else {
        Ok(())
    }
}

pub fn validate_url(value: &str, field_name: &str) -> ConfigResult<()> {
    if !value.starts_with("http://") && !value.starts_with("https://") {
        Err(ConfigError::Validation(format!(
            "{field_name} must be a valid URL (http:// or https://), got {value}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_email(value: &str, field_name: &str) -> ConfigResult<()> {
    if !EMAIL_REGEX.is_match(value) {
        Err(ConfigError::Validation(format!(
            "{field_name} must be a valid email"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_pattern(
    value: &str,
    field_name: &str,
    pattern: &regex::Regex,
    pattern_desc: Option<&str>,
) -> ConfigResult<()> {
    if !pattern.is_match(value) {
        Err(ConfigError::Validation(format!(
            "{field_name} must match pattern: {}",
            pattern_desc.unwrap_or_else(|| pattern.as_str())
        )))
    } else {
        Ok(())
    }
}

pub fn validate_length(
    value: &str,
    field_name: &str,
    min: Option<usize>,
    max: Option<usize>,
) -> ConfigResult<()> {
    let len = value.len();
    if let Some(min_len) = min {
        if len < min_len {
            return Err(ConfigError::Validation(format!(
                "{field_name} must be at least {min_len} characters"
            )));
        }
    }
    if let Some(max_len) = max {
        if len > max_len {
            return Err(ConfigError::Validation(format!(
                "{field_name} must be at most {max_len} characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_not_empty(value: &str, field_name: &str) -> ConfigResult<()> {
    if value.trim().is_empty() {
        Err(ConfigError::Validation(format!(
            "{field_name} cannot be empty"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_array_not_empty<T>(value: &[T], field_name: &str) -> ConfigResult<()> {
    if value.is_empty() {
        Err(ConfigError::Validation(format!(
            "{field_name} cannot be empty"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_no_intersection(
    a: &[String],
    b: &[String],
    field_a_name: &str,
    field_b_name: &str,
) -> ConfigResult<()> {
    let intersection: Vec<_> = a.iter().filter(|x| b.contains(x)).collect();
    if !intersection.is_empty() {
        Err(ConfigError::Validation(format!(
            "{field_a_name} and {field_b_name} must not intersect, found: {:?}",
            intersection
        )))
    } else {
        Ok(())
    }
}

pub fn validate_all(results: Vec<ConfigResult<()>>) -> ConfigResult<()> {
    let errors: Vec<String> = results
        .into_iter()
        .filter_map(|r| r.err())
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation(errors.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_required() {
        assert!(validate_required("hello", "name").is_ok());
        assert!(validate_required("", "name").is_err());
    }

    #[test]
    fn test_validate_range() {
        assert!(validate_range(50, 0, 100, "value").is_ok());
        assert!(validate_range(0, 0, 100, "value").is_ok());
        assert!(validate_range(100, 0, 100, "value").is_ok());
        assert!(validate_range(101, 0, 100, "value").is_err());
        assert!(validate_range(-1i64, 0, 100, "value").is_err());
    }

    #[test]
    fn test_validate_enum() {
        assert!(validate_enum("auto", &["auto", "always", "never"], "mode").is_ok());
        assert!(validate_enum("invalid", &["auto", "always", "never"], "mode").is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com", "url").is_ok());
        assert!(validate_url("http://localhost:3000", "url").is_ok());
        assert!(validate_url("ftp://example.com", "url").is_err());
    }

    #[test]
    fn test_validate_no_intersection() {
        let a = vec!["a".to_string(), "b".to_string()];
        let b = vec!["c".to_string(), "d".to_string()];
        assert!(validate_no_intersection(&a, &b, "a", "b").is_ok());

        let b = vec!["b".to_string(), "c".to_string()];
        assert!(validate_no_intersection(&a, &b, "a", "b").is_err());
    }

    #[test]
    fn test_validate_all() {
        assert!(validate_all(vec![Ok(()), Ok(())]).is_ok());
        assert!(
            validate_all(vec![Ok(()), Err(ConfigError::Validation("fail".to_string()))]).is_err()
        );
    }

    #[test]
    fn test_validate_email() {
        assert!(validate_email("user@example.com", "email").is_ok());
        assert!(validate_email("user.name+tag@domain.co.uk", "email").is_ok());
        assert!(validate_email("invalid", "email").is_err());
        assert!(validate_email("no-at-sign", "email").is_err());
        assert!(validate_email("@no-local.org", "email").is_err());
    }

    #[test]
    fn test_validate_pattern() {
        let regex = regex::Regex::new(r"^[a-z][a-z0-9_]*$").unwrap();
        assert!(validate_pattern("valid_name", "id", &regex, None).is_ok());
        assert!(validate_pattern("ValidName", "id", &regex, None).is_err());
        assert!(validate_pattern("123_start", "id", &regex, None).is_err());

        let desc = Some("lowercase identifier");
        assert!(validate_pattern("Bad", "id", &regex, desc).is_err());
    }

    #[test]
    fn test_validate_length() {
        assert!(validate_length("hello", "name", None, None).is_ok());
        assert!(validate_length("hi", "name", Some(2), Some(5)).is_ok());
        assert!(validate_length("a", "name", Some(2), Some(5)).is_err());
        assert!(validate_length("toolong", "name", Some(2), Some(5)).is_err());
        assert!(validate_length("abc", "name", Some(1), None).is_ok());
        assert!(validate_length("abc", "name", None, Some(5)).is_ok());
    }

    #[test]
    fn test_validate_not_empty() {
        assert!(validate_not_empty("hello", "field").is_ok());
        assert!(validate_not_empty("", "field").is_err());
        assert!(validate_not_empty("   ", "field").is_err());
    }

    #[test]
    fn test_validate_array_not_empty() {
        let items = vec![1, 2, 3];
        assert!(validate_array_not_empty(&items, "items").is_ok());
        let empty: Vec<i32> = vec![];
        assert!(validate_array_not_empty(&empty, "items").is_err());
    }
}
