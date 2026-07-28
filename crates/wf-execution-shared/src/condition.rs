pub mod cache;

pub use cache::{ConditionCache, ConditionCacheConfig};

use std::collections::HashMap;

use serde_json::Value;

use crate::error::ExecutionSharedResult;

pub struct ConditionEvaluator;

impl ConditionEvaluator {
    pub fn normalize_condition(condition: &str) -> String {
        condition.replace("===", "==").replace("!==", "!=")
    }

    pub fn evaluate(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let condition = Self::normalize_condition(condition);
        let condition = condition.trim();

        if condition.starts_with("eq(") {
            Self::eval_eq(condition, context)
        } else if condition.starts_with("ne(") {
            Self::eval_ne(condition, context)
        } else if condition.starts_with("gt(") {
            Self::eval_gt(condition, context)
        } else if condition.starts_with("lt(") {
            Self::eval_lt(condition, context)
        } else if condition.starts_with("and(") {
            Self::eval_and(condition, context)
        } else if condition.starts_with("or(") {
            Self::eval_or(condition, context)
        } else if condition.starts_with("not(") {
            Self::eval_not(condition, context)
        } else {
            Self::eval_exists(condition, context)
        }
    }

    pub fn evaluate_with_cache(
        condition: &str,
        context: &HashMap<String, Value>,
        cache: &ConditionCache,
    ) -> ExecutionSharedResult<bool> {
        if let Some(result) = cache.get_execution_result(condition, context) {
            return Ok(result);
        }

        let result = Self::evaluate(condition, context)?;
        cache.put_execution_result(condition, context, result);
        Ok(result)
    }

    pub fn generate_cache_key(condition: &str) -> String {
        let normalized = Self::normalize_condition(condition);
        let hash = blake3::hash(normalized.as_bytes());
        hash.to_hex().to_string()
    }

    fn parse_args(input: &str) -> Vec<&str> {
        let inner = input.trim();
        let inner = if let Some(stripped) = inner.strip_suffix(')') {
            stripped
        } else {
            inner
        };

        let mut args = Vec::new();
        let mut depth = 0;
        let mut start = 0;

        for (i, c) in inner.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(&inner[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }

        if start < inner.len() {
            args.push(&inner[start..]);
        }

        args
    }

    fn resolve_value(val: &str, context: &HashMap<String, Value>) -> Value {
        let val = val.trim();

        if (val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')) {
            return Value::String(val[1..val.len() - 1].to_string());
        }

        if val == "true" {
            return Value::Bool(true);
        }
        if val == "false" {
            return Value::Bool(false);
        }
        if val == "null" {
            return Value::Null;
        }

        if let Ok(n) = val.parse::<i64>() {
            return Value::Number(n.into());
        }
        if let Ok(f) = val.parse::<f64>() {
            return serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null);
        }

        if let Some(v) = Self::lookup_variable(val, context) {
            return v;
        }

        Value::String(val.to_string())
    }

    fn lookup_variable(path: &str, context: &HashMap<String, Value>) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let first = parts.first()?;

        let mut current = context.get(*first)?.clone();

        for part in &parts[1..] {
            if let Value::Object(map) = &current {
                current = map.get(*part)?.clone();
            } else {
                return None;
            }
        }

        Some(current)
    }

    fn eval_eq(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(crate::error::ExecutionSharedError::ConditionError(
                format!("eq() expects 2 args, got {}", args.len()),
            ));
        }
        Ok(Self::resolve_value(args[0], context) == Self::resolve_value(args[1], context))
    }

    fn eval_ne(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(crate::error::ExecutionSharedError::ConditionError(
                format!("ne() expects 2 args, got {}", args.len()),
            ));
        }
        Ok(Self::resolve_value(args[0], context) != Self::resolve_value(args[1], context))
    }

    fn eval_gt(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(crate::error::ExecutionSharedError::ConditionError(
                format!("gt() expects 2 args, got {}", args.len()),
            ));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a > b),
            _ => Err(crate::error::ExecutionSharedError::ConditionError(
                "gt() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_lt(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(crate::error::ExecutionSharedError::ConditionError(
                format!("lt() expects 2 args, got {}", args.len()),
            ));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a < b),
            _ => Err(crate::error::ExecutionSharedError::ConditionError(
                "lt() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_and(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[4..]);
        for arg in &args {
            if !Self::resolve_bool(arg, context)? {
                return Ok(false);
            }
        }
        Ok(!args.is_empty())
    }

    fn eval_or(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        for arg in &args {
            if Self::resolve_bool(arg, context)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn eval_not(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let args = Self::parse_args(&condition[4..]);
        if args.len() != 1 {
            return Err(crate::error::ExecutionSharedError::ConditionError(
                format!("not() expects 1 arg, got {}", args.len()),
            ));
        }
        Ok(!Self::resolve_bool(args[0], context)?)
    }

    fn eval_exists(condition: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let var_name = condition.trim();
        match Self::lookup_variable(var_name, context) {
            Some(Value::Null) | None => Ok(false),
            Some(Value::Bool(b)) => Ok(b),
            Some(Value::String(s)) => Ok(!s.is_empty()),
            Some(Value::Number(n)) => Ok(n.as_f64().map(|n| n != 0.0).unwrap_or(false)),
            Some(_) => Ok(true),
        }
    }

    fn resolve_bool(val: &str, context: &HashMap<String, Value>) -> ExecutionSharedResult<bool> {
        let val = val.trim();

        if val.starts_with("eq(") || val.starts_with("ne(") ||
           val.starts_with("gt(") || val.starts_with("lt(") ||
           val.starts_with("and(") || val.starts_with("or(") ||
           val.starts_with("not(") {
            return Self::evaluate(val, context);
        }

        let resolved = Self::resolve_value(val, context);
        match resolved {
            Value::Bool(b) => Ok(b),
            Value::Number(n) => Ok(n.as_f64().map(|n| n != 0.0).unwrap_or(false)),
            Value::String(s) => Ok(!s.is_empty() && s != "false"),
            Value::Null => Ok(false),
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(vars: &[(&str, Value)]) -> HashMap<String, Value> {
        vars.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn test_eq_strings() {
        let ctx = ctx(&[]);
        assert!(ConditionEvaluator::evaluate(r#"eq("hello", "hello")"#, &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate(r#"eq("hello", "world")"#, &ctx).unwrap());
    }

    #[test]
    fn test_eq_variables() {
        let ctx = ctx(&[("status", Value::String("active".to_string()))]);
        assert!(ConditionEvaluator::evaluate("eq(status, \"active\")", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("eq(status, \"inactive\")", &ctx).unwrap());
    }

    #[test]
    fn test_gt() {
        let ctx = ctx(&[("count", Value::Number(10.into()))]);
        assert!(ConditionEvaluator::evaluate("gt(count, 5)", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("gt(count, 15)", &ctx).unwrap());
    }

    #[test]
    fn test_and() {
        let ctx = ctx(&[
            ("a", Value::Bool(true)),
            ("b", Value::Bool(true)),
        ]);
        assert!(ConditionEvaluator::evaluate("and(eq(a, true), eq(b, true))", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("and(eq(a, true), eq(b, false))", &ctx).unwrap());
    }

    #[test]
    fn test_or() {
        let ctx = ctx(&[
            ("a", Value::Bool(false)),
            ("b", Value::Bool(true)),
        ]);
        assert!(ConditionEvaluator::evaluate("or(eq(a, true), eq(b, true))", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("or(eq(a, true), eq(b, false))", &ctx).unwrap());
    }

    #[test]
    fn test_not() {
        let ctx = ctx(&[("flag", Value::Bool(false))]);
        assert!(ConditionEvaluator::evaluate("not(eq(flag, true))", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("not(eq(flag, false))", &ctx).unwrap());
    }

    #[test]
    fn test_exists() {
        let ctx = ctx(&[("name", Value::String("test".to_string()))]);
        assert!(ConditionEvaluator::evaluate("name", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("missing", &ctx).unwrap());
    }

    #[test]
    fn test_normalize_triple_equals() {
        let normalized = ConditionEvaluator::normalize_condition("a === b");
        assert_eq!(normalized, "a == b");
    }

    #[test]
    fn test_normalize_triple_not_equals() {
        let normalized = ConditionEvaluator::normalize_condition("a !== b");
        assert_eq!(normalized, "a != b");
    }

    #[test]
    fn test_normalize_mixed() {
        let normalized = ConditionEvaluator::normalize_condition("a === b && c !== d");
        assert_eq!(normalized, "a == b && c != d");
    }

    #[test]
    fn test_normalize_no_change() {
        let normalized = ConditionEvaluator::normalize_condition("eq(a, b)");
        assert_eq!(normalized, "eq(a, b)");
    }

    #[test]
    fn test_generate_cache_key_deterministic() {
        let key1 = ConditionEvaluator::generate_cache_key("eq(status, \"active\")");
        let key2 = ConditionEvaluator::generate_cache_key("eq(status, \"active\")");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_generate_cache_key_different_conditions() {
        let key1 = ConditionEvaluator::generate_cache_key("eq(a, b)");
        let key2 = ConditionEvaluator::generate_cache_key("eq(c, d)");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_generate_cache_key_normalized() {
        let key1 = ConditionEvaluator::generate_cache_key("a === b");
        let key2 = ConditionEvaluator::generate_cache_key("a == b");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_evaluate_with_cache() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = ctx(&[("x", Value::Number(42.into()))]);

        let r1 = ConditionEvaluator::evaluate_with_cache("gt(x, 10)", &ctx, &cache).unwrap();
        assert!(r1);

        let r2 = ConditionEvaluator::evaluate_with_cache("gt(x, 10)", &ctx, &cache).unwrap();
        assert!(r2);
    }
}
