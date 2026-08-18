use wf_types::script::sandbox::{SecuritySeverity, SecurityViolation};

pub struct SecurityValidator;

const MAX_EXPRESSION_LENGTH: usize = 1000;
const MAX_EXPRESSION_DEPTH: usize = 10;
const FORBIDDEN_PROPERTIES: &[&str] = &["__proto__", "constructor", "prototype"];

impl SecurityValidator {
    pub fn validate_expression(expr: &str) -> Vec<SecurityViolation> {
        let mut violations = Vec::new();

        if expr.len() > MAX_EXPRESSION_LENGTH {
            violations.push(SecurityViolation {
                field: "expression".to_string(),
                reason: format!("Expression exceeds max length of {}", MAX_EXPRESSION_LENGTH),
                severity: SecuritySeverity::Error,
            });
            return violations;
        }

        for &prop in FORBIDDEN_PROPERTIES {
            if expr.contains(prop) {
                violations.push(SecurityViolation {
                    field: "expression".to_string(),
                    reason: format!("Expression contains forbidden property: {}", prop),
                    severity: SecuritySeverity::Critical,
                });
            }
        }

        if expr.contains("..") {
            violations.push(SecurityViolation {
                field: "expression".to_string(),
                reason: "Expression contains consecutive dots".to_string(),
                severity: SecuritySeverity::Error,
            });
        }

        let depth = Self::compute_depth(expr);
        if depth > MAX_EXPRESSION_DEPTH {
            violations.push(SecurityViolation {
                field: "expression".to_string(),
                reason: format!(
                    "Expression depth {} exceeds limit of {}",
                    depth, MAX_EXPRESSION_DEPTH
                ),
                severity: SecuritySeverity::Error,
            });
        }

        violations
    }

    pub fn validate_path(path: &str) -> Vec<SecurityViolation> {
        let mut violations = Vec::new();

        if path.is_empty() {
            violations.push(SecurityViolation {
                field: "path".to_string(),
                reason: "Path is empty".to_string(),
                severity: SecuritySeverity::Error,
            });
            return violations;
        }

        if path.contains("..") {
            violations.push(SecurityViolation {
                field: "path".to_string(),
                reason: "Path contains directory traversal (..)".to_string(),
                severity: SecuritySeverity::Critical,
            });
        }

        if path.contains("//") {
            violations.push(SecurityViolation {
                field: "path".to_string(),
                reason: "Path contains empty component (//)".to_string(),
                severity: SecuritySeverity::Warning,
            });
        }

        if path.contains('\0') {
            violations.push(SecurityViolation {
                field: "path".to_string(),
                reason: "Path contains null byte".to_string(),
                severity: SecuritySeverity::Critical,
            });
        }

        violations
    }

    pub fn validate_array_index(index: i64, length: usize) -> Vec<SecurityViolation> {
        let mut violations = Vec::new();

        if index < 0 {
            violations.push(SecurityViolation {
                field: "array_index".to_string(),
                reason: "Array index is negative".to_string(),
                severity: SecuritySeverity::Error,
            });
        }

        if index as usize >= length {
            violations.push(SecurityViolation {
                field: "array_index".to_string(),
                reason: format!("Array index {} out of bounds for length {}", index, length),
                severity: SecuritySeverity::Error,
            });
        }

        violations
    }

    pub fn validate_value_type(value: &serde_json::Value) -> Vec<SecurityViolation> {
        let mut violations = Vec::new();

        match value {
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_) => {}
            serde_json::Value::Array(arr) => {
                for item in arr {
                    violations.extend(Self::validate_value_type(item));
                }
            }
            serde_json::Value::Object(obj) => {
                for (_key, val) in obj {
                    violations.extend(Self::validate_value_type(val));
                }
            }
        }

        violations
    }

    fn compute_depth(expr: &str) -> usize {
        let mut depth = 0usize;
        let mut max_depth = 0usize;

        for ch in expr.chars() {
            match ch {
                '(' | '[' | '{' => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }

        max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_simple_expression() {
        let violations = SecurityValidator::validate_expression("foo.bar");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_validate_expression_too_long() {
        let long = "a".repeat(1001);
        let violations = SecurityValidator::validate_expression(&long);
        assert!(!violations.is_empty());
        assert_eq!(violations[0].severity, SecuritySeverity::Error);
    }

    #[test]
    fn test_validate_expression_prototype_pollution() {
        let violations = SecurityValidator::validate_expression("a.__proto__");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].severity, SecuritySeverity::Critical);
    }

    #[test]
    fn test_validate_expression_consecutive_dots() {
        let violations = SecurityValidator::validate_expression("a..b");
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_validate_path_traversal() {
        let violations = SecurityValidator::validate_path("../../../etc/passwd");
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_validate_path_null_byte() {
        let violations = SecurityValidator::validate_path("/safe\0/evil");
        assert!(!violations.is_empty());
        assert_eq!(violations[0].severity, SecuritySeverity::Critical);
    }

    #[test]
    fn test_validate_path_empty() {
        let violations = SecurityValidator::validate_path("");
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_validate_value_type_safe() {
        let val = serde_json::json!({"name": "hello", "count": 42});
        let violations = SecurityValidator::validate_value_type(&val);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_validate_array_index_negative() {
        let violations = SecurityValidator::validate_array_index(-1, 5);
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_validate_array_index_out_of_bounds() {
        let violations = SecurityValidator::validate_array_index(10, 5);
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_validate_array_index_valid() {
        let violations = SecurityValidator::validate_array_index(3, 10);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_expression_depth() {
        let deep = "a[b[c[d[e[f[g]]]]]]";
        let violations = SecurityValidator::validate_expression(deep);
        assert!(
            violations.is_empty(),
            "depth 7 should be within limit 10: {:?}",
            violations
        );

        let too_deep = "a(b(c(d(e(f(g(h(i(j(k(l(m(n(o(p))))))))))))))";
        let violations = SecurityValidator::validate_expression(too_deep);
        assert!(!violations.is_empty(), "depth > 10 should be flagged");
    }
}
