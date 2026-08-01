use std::collections::HashMap;
use std::time::Duration;

use moka::sync::Cache as MokaCache;
use serde_json::Value;

use crate::error::{CoreError, CoreResult};

#[derive(Debug, Clone)]
pub struct ConditionCacheConfig {
    pub max_compilation_entries: u64,
    pub max_execution_entries: u64,
    pub time_to_live: Option<Duration>,
}

impl Default for ConditionCacheConfig {
    fn default() -> Self {
        Self {
            max_compilation_entries: 10_000,
            max_execution_entries: 10_000,
            time_to_live: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CompilationKey(String);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ExecutionKey {
    condition_hash: String,
    context_hash: String,
}

pub struct ConditionCache {
    compilation_cache: MokaCache<CompilationKey, ()>,
    execution_cache: MokaCache<ExecutionKey, bool>,
}

impl ConditionCache {
    pub fn new(config: ConditionCacheConfig) -> Self {
        let compilation_cache = MokaCache::builder()
            .max_capacity(config.max_compilation_entries)
            .time_to_live(config.time_to_live.unwrap_or(Duration::from_secs(3600)))
            .build();

        let execution_cache = MokaCache::builder()
            .max_capacity(config.max_execution_entries)
            .time_to_live(config.time_to_live.unwrap_or(Duration::from_secs(3600)))
            .build();

        Self {
            compilation_cache,
            execution_cache,
        }
    }

    pub fn check_compilation_cache(&self, condition: &str) -> bool {
        let key = CompilationKey(condition.to_string());
        self.compilation_cache.contains_key(&key)
    }

    pub fn record_compilation(&self, condition: &str) {
        let key = CompilationKey(condition.to_string());
        self.compilation_cache.insert(key, ());
    }

    pub fn get_execution_result(
        &self,
        condition: &str,
        context: &HashMap<String, Value>,
    ) -> Option<bool> {
        let context_hash = hash_context(context);
        let key = ExecutionKey {
            condition_hash: condition.to_string(),
            context_hash,
        };
        self.execution_cache.get(&key)
    }

    pub fn put_execution_result(
        &self,
        condition: &str,
        context: &HashMap<String, Value>,
        result: bool,
    ) {
        let context_hash = hash_context(context);
        let key = ExecutionKey {
            condition_hash: condition.to_string(),
            context_hash,
        };
        self.execution_cache.insert(key, result);
    }

    pub fn invalidate(&self, condition: &str) {
        self.compilation_cache
            .invalidate(&CompilationKey(condition.to_string()));
        let to_remove: Vec<_> = self
            .execution_cache
            .iter()
            .filter(|(k, _)| k.condition_hash == condition)
            .map(|(k, _)| (*k).clone())
            .collect();
        for key in to_remove {
            self.execution_cache.invalidate(&key);
        }
    }

    pub fn clear(&self) {
        self.compilation_cache.invalidate_all();
        self.execution_cache.invalidate_all();
    }

    pub fn entry_count(&self) -> usize {
        self.compilation_cache.entry_count() as usize + self.execution_cache.entry_count() as usize
    }
}

fn hash_context(context: &HashMap<String, Value>) -> String {
    let serialized = serde_json::to_string(context).unwrap_or_default();
    let hash = blake3::hash(serialized.as_bytes());
    hash.to_hex().to_string()
}

pub struct ConditionEvaluator;

impl ConditionEvaluator {
    pub fn normalize_condition(condition: &str) -> String {
        condition.replace("===", "==").replace("!==", "!=")
    }

    pub fn evaluate(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let condition = Self::normalize_condition(condition);
        let condition = Self::interpolate(&condition, context);
        let condition = condition.trim();

        if condition.starts_with("eq(") {
            Self::eval_eq(condition, context)
        } else if condition.starts_with("ne(") {
            Self::eval_ne(condition, context)
        } else if condition.starts_with("gt(") {
            Self::eval_gt(condition, context)
        } else if condition.starts_with("lt(") {
            Self::eval_lt(condition, context)
        } else if condition.starts_with("ge(") {
            Self::eval_ge(condition, context)
        } else if condition.starts_with("le(") {
            Self::eval_le(condition, context)
        } else if condition.starts_with("and(") {
            Self::eval_and(condition, context)
        } else if condition.starts_with("or(") {
            Self::eval_or(condition, context)
        } else if condition.starts_with("not(") {
            Self::eval_not(condition, context)
        } else if condition.starts_with("isNull(") {
            Self::eval_is_null(condition, context)
        } else if condition.starts_with("isEmpty(") {
            Self::eval_is_empty(condition, context)
        } else if condition.starts_with("isTrue(") {
            Self::eval_is_true(condition, context)
        } else if condition.starts_with("isFalse(") {
            Self::eval_is_false(condition, context)
        } else if condition.starts_with("hasValue(") {
            Self::eval_has_value(condition, context)
        } else if condition.starts_with("contains(") {
            Self::eval_contains(condition, context)
        } else if condition.starts_with("startsWith(") {
            Self::eval_starts_with(condition, context)
        } else if condition.starts_with("endsWith(") {
            Self::eval_ends_with(condition, context)
        } else if condition.starts_with("length(") {
            Self::eval_length(condition, context)
        } else {
            Self::eval_exists(condition, context)
        }
    }

    /// Replace `${path}` / `{{path}}` occurrences with the resolved values
    /// from the context. Unresolved paths are left as-is.
    fn interpolate(condition: &str, context: &HashMap<String, Value>) -> String {
        let mut result = String::with_capacity(condition.len());
        let bytes = condition.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = condition[i + 2..].find('}') {
                    let path = &condition[i + 2..i + 2 + end];
                    if let Some(value) = Self::lookup_variable(path, context) {
                        result.push_str(&Self::value_to_literal(&value));
                        i += 2 + end + 1;
                        continue;
                    }
                }
            }
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = condition[i + 2..].find("}}") {
                    let path = &condition[i + 2..i + 2 + end];
                    if let Some(value) = Self::lookup_variable(path, context) {
                        result.push_str(&Self::value_to_literal(&value));
                        i += 2 + end + 2;
                        continue;
                    }
                }
            }
            result.push(condition[i..].chars().next().unwrap());
            i += condition[i..].chars().next().unwrap().len_utf8();
        }
        result
    }

    fn value_to_literal(value: &Value) -> String {
        match value {
            Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            Value::Null => "null".to_string(),
            other => other.to_string(),
        }
    }

    pub fn evaluate_with_cache(
        condition: &str,
        context: &HashMap<String, Value>,
        cache: &ConditionCache,
    ) -> CoreResult<bool> {
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

        if (val.starts_with('"') && val.ends_with('"'))
            || (val.starts_with('\'') && val.ends_with('\''))
        {
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
            return serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null);
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

    fn eval_eq(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "eq() expects 2 args, got {}",
                args.len()
            )));
        }
        Ok(Self::resolve_value(args[0], context) == Self::resolve_value(args[1], context))
    }

    fn eval_ne(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "ne() expects 2 args, got {}",
                args.len()
            )));
        }
        Ok(Self::resolve_value(args[0], context) != Self::resolve_value(args[1], context))
    }

    fn eval_gt(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "gt() expects 2 args, got {}",
                args.len()
            )));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a > b),
            _ => Err(CoreError::ConditionError(
                "gt() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_lt(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "lt() expects 2 args, got {}",
                args.len()
            )));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a < b),
            _ => Err(CoreError::ConditionError(
                "lt() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_ge(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "ge() expects 2 args, got {}",
                args.len()
            )));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a >= b),
            _ => Err(CoreError::ConditionError(
                "ge() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_le(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "le() expects 2 args, got {}",
                args.len()
            )));
        }
        let a = Self::resolve_value(args[0], context);
        let b = Self::resolve_value(args[1], context);
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => Ok(a <= b),
            _ => Err(CoreError::ConditionError(
                "le() requires numeric arguments".to_string(),
            )),
        }
    }

    fn eval_is_null(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[7..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "isNull() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(matches!(Self::resolve_value(args[0], context), Value::Null))
    }

    fn eval_is_empty(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[8..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "isEmpty() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(match Self::resolve_value(args[0], context) {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(a) => a.is_empty(),
            Value::Object(o) => o.is_empty(),
            _ => false,
        })
    }

    fn eval_is_true(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[7..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "isTrue() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(matches!(Self::resolve_value(args[0], context), Value::Bool(true)))
    }

    fn eval_is_false(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[8..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "isFalse() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(matches!(
            Self::resolve_value(args[0], context),
            Value::Bool(false)
        ))
    }

    fn eval_has_value(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[9..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "hasValue() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(!matches!(Self::resolve_value(args[0], context), Value::Null))
    }

    fn eval_contains(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[9..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "contains() expects 2 args, got {}",
                args.len()
            )));
        }
        match (Self::resolve_value(args[0], context), Self::resolve_value(args[1], context)) {
            (Value::String(s), Value::String(sub)) => Ok(s.contains(&sub)),
            (Value::Array(a), needle) => Ok(a.contains(&needle)),
            _ => Err(CoreError::ConditionError(
                "contains() requires a string or array first argument".to_string(),
            )),
        }
    }

    fn eval_starts_with(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[11..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "startsWith() expects 2 args, got {}",
                args.len()
            )));
        }
        match (Self::resolve_value(args[0], context), Self::resolve_value(args[1], context)) {
            (Value::String(s), Value::String(prefix)) => Ok(s.starts_with(&prefix)),
            _ => Err(CoreError::ConditionError(
                "startsWith() requires string arguments".to_string(),
            )),
        }
    }

    fn eval_ends_with(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[9..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "endsWith() expects 2 args, got {}",
                args.len()
            )));
        }
        match (Self::resolve_value(args[0], context), Self::resolve_value(args[1], context)) {
            (Value::String(s), Value::String(suffix)) => Ok(s.ends_with(&suffix)),
            _ => Err(CoreError::ConditionError(
                "endsWith() requires string arguments".to_string(),
            )),
        }
    }

    fn eval_length(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[7..]);
        if args.len() != 2 {
            return Err(CoreError::ConditionError(format!(
                "length() expects 2 args, got {}",
                args.len()
            )));
        }
        let len = match Self::resolve_value(args[0], context) {
            Value::String(s) => Some(s.chars().count() as f64),
            Value::Array(a) => Some(a.len() as f64),
            Value::Object(o) => Some(o.len() as f64),
            _ => None,
        };
        let expected = Self::resolve_value(args[1], context);
        match (len, expected.as_f64()) {
            (Some(len), Some(expected)) => Ok(len == expected),
            _ => Err(CoreError::ConditionError(
                "length() requires a string/array/object and a number".to_string(),
            )),
        }
    }

    fn eval_and(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[4..]);
        for arg in &args {
            if !Self::resolve_bool(arg, context)? {
                return Ok(false);
            }
        }
        Ok(!args.is_empty())
    }

    fn eval_or(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[3..]);
        for arg in &args {
            if Self::resolve_bool(arg, context)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn eval_not(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let args = Self::parse_args(&condition[4..]);
        if args.len() != 1 {
            return Err(CoreError::ConditionError(format!(
                "not() expects 1 arg, got {}",
                args.len()
            )));
        }
        Ok(!Self::resolve_bool(args[0], context)?)
    }

    fn eval_exists(condition: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let var_name = condition.trim();
        match Self::lookup_variable(var_name, context) {
            Some(Value::Null) | None => Ok(false),
            Some(Value::Bool(b)) => Ok(b),
            Some(Value::String(s)) => Ok(!s.is_empty()),
            Some(Value::Number(n)) => Ok(n.as_f64().map(|n| n != 0.0).unwrap_or(false)),
            Some(_) => Ok(true),
        }
    }

    fn resolve_bool(val: &str, context: &HashMap<String, Value>) -> CoreResult<bool> {
        let val = val.trim();

        if val.starts_with("eq(")
            || val.starts_with("ne(")
            || val.starts_with("gt(")
            || val.starts_with("lt(")
            || val.starts_with("ge(")
            || val.starts_with("le(")
            || val.starts_with("and(")
            || val.starts_with("or(")
            || val.starts_with("not(")
            || val.starts_with("isNull(")
            || val.starts_with("isEmpty(")
            || val.starts_with("isTrue(")
            || val.starts_with("isFalse(")
            || val.starts_with("hasValue(")
            || val.starts_with("contains(")
            || val.starts_with("startsWith(")
            || val.starts_with("endsWith(")
            || val.starts_with("length(")
        {
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
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
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
        let ctx = ctx(&[("a", Value::Bool(true)), ("b", Value::Bool(true))]);
        assert!(ConditionEvaluator::evaluate("and(eq(a, true), eq(b, true))", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("and(eq(a, true), eq(b, false))", &ctx).unwrap());
    }

    #[test]
    fn test_or() {
        let ctx = ctx(&[("a", Value::Bool(false)), ("b", Value::Bool(true))]);
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
    fn test_predicates() {
        let ctx = ctx(&[
            ("missing_var", Value::Null),
            ("empty_str", Value::String("".to_string())),
            ("flag", Value::Bool(true)),
            ("off", Value::Bool(false)),
            ("list", serde_json::json!([1, 2, 3])),
            ("text", Value::String("hello world".to_string())),
        ]);
        assert!(ConditionEvaluator::evaluate("isNull(missing_var)", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("isNull(flag)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("isEmpty(empty_str)", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("isEmpty(text)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("isTrue(flag)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("isFalse(off)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("hasValue(flag)", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("hasValue(missing_var)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"contains("hello world", "world")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"contains(list, 2)"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"startsWith("hello world", "hello")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"endsWith("hello world", "world")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("length(list, 3)", &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("ge(list_none, 3)", &ctx).is_err());
    }

    #[test]
    fn test_nested_predicates() {
        let ctx = ctx(&[
            ("a", Value::String("".to_string())),
            ("b", Value::Bool(true)),
            ("missing", Value::Null),
        ]);
        assert!(ConditionEvaluator::evaluate("and(isEmpty(a), isTrue(b))", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate("or(hasValue(missing), isFalse(b))", &ctx).unwrap());
    }

    #[test]
    fn test_interpolation() {
        let ctx = ctx(&[
            ("status", Value::String("active".to_string())),
            ("count", Value::Number(5.into())),
            ("nested", serde_json::json!({"level": "high"})),
        ]);
        assert!(ConditionEvaluator::evaluate(r#"eq(${status}, "active")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate("gt(${count}, 3)", &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate(r#"eq(${count}, "active")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"eq(${nested.level}, "high")"#, &ctx).unwrap());
        assert!(ConditionEvaluator::evaluate(r#"eq({{status}}, "active")"#, &ctx).unwrap());
        assert!(!ConditionEvaluator::evaluate(r#"eq(${unknown}, "active")"#, &ctx).unwrap());
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

    #[test]
    fn test_compilation_cache() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        assert!(!cache.check_compilation_cache("eq(a, b)"));
        cache.record_compilation("eq(a, b)");
        assert!(cache.check_compilation_cache("eq(a, b)"));
    }

    #[test]
    fn test_execution_cache() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        assert!(cache.get_execution_result("eq(a, b)", &ctx).is_none());
        cache.put_execution_result("eq(a, b)", &ctx, true);
        assert_eq!(cache.get_execution_result("eq(a, b)", &ctx), Some(true));
    }

    #[test]
    fn test_invalidate() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        cache.record_compilation("eq(a, b)");
        cache.put_execution_result("eq(a, b)", &ctx, true);

        cache.invalidate("eq(a, b)");

        assert!(!cache.check_compilation_cache("eq(a, b)"));
        assert!(cache.get_execution_result("eq(a, b)", &ctx).is_none());
    }

    #[test]
    fn test_clear() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let ctx = HashMap::new();

        cache.record_compilation("eq(a, b)");
        cache.record_compilation("eq(c, d)");
        cache.put_execution_result("eq(a, b)", &ctx, true);

        cache.clear();
        assert!(!cache.check_compilation_cache("eq(a, b)"));
        assert!(!cache.check_compilation_cache("eq(c, d)"));
    }

    #[test]
    fn test_different_contexts_different_results() {
        let cache = ConditionCache::new(ConditionCacheConfig::default());
        let mut ctx1 = HashMap::new();
        ctx1.insert("x".to_string(), Value::Number(1.into()));
        let mut ctx2 = HashMap::new();
        ctx2.insert("x".to_string(), Value::Number(2.into()));

        cache.put_execution_result("gt(x, 0)", &ctx1, true);
        cache.put_execution_result("gt(x, 0)", &ctx2, true);

        assert_eq!(cache.get_execution_result("gt(x, 0)", &ctx1), Some(true));
        assert_eq!(cache.get_execution_result("gt(x, 0)", &ctx2), Some(true));
    }
}
