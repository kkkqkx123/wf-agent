use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;

pub type VariableStore = Arc<DashMap<String, Value>>;

pub fn create_variable_store() -> VariableStore {
    Arc::new(DashMap::new())
}

/// Error surfaced by variable expression evaluation.
#[derive(Debug, Clone)]
pub struct ExpressionError {
    pub expression: String,
    pub message: String,
}

impl ExpressionError {
    pub fn new(expression: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expression '{}': {}", self.expression, self.message)
    }
}

impl std::error::Error for ExpressionError {}

/// Small recursive-descent expression evaluator for VARIABLE nodes.
///
/// Supports variable references (`${path}`), arithmetic (`+ - * / %`),
/// comparison (`== != > < >= <=`), logic (`&& || !`), ternary
/// (`cond ? a : b`), unary minus and parenthesised grouping. Values resolve
/// against the variable store; unquoted identifiers are resolved as
/// variables, so `a + 1` reads `a` from the store.
pub struct ExprEvaluator<'a> {
    input: &'a [u8],
    pos: usize,
    variables: &'a VariableStore,
}

impl<'a> ExprEvaluator<'a> {
    pub fn new(input: &'a str, variables: &'a VariableStore) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            variables,
        }
    }

    /// Evaluate a full expression, failing on trailing tokens.
    pub fn evaluate(mut self) -> Result<Value, ExpressionError> {
        let value = self.parse_ternary()?;
        self.skip_whitespace();
        if self.pos < self.input.len() {
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unexpected trailing tokens",
            ));
        }
        Ok(value)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        self.skip_whitespace();
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), ExpressionError> {
        if self.eat(b) {
            Ok(())
        } else {
            Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                format!("expected '{}'", b as char),
            ))
        }
    }

    /// Two-char operator helper: matches `prefix` when the following byte is
    /// `second`, else falls back to the single-char `prefix`.
    fn eat_op(&mut self, prefix: u8, second: u8) -> Option<()> {
        self.skip_whitespace();
        if self.peek() == Some(prefix) {
            if self.input.get(self.pos + 1) == Some(&second) {
                self.pos += 2;
                Some(())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn eat_single(&mut self, b: u8) -> bool {
        self.skip_whitespace();
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    // ternary := or ('?' ternary ':' ternary)?
    fn parse_ternary(&mut self) -> Result<Value, ExpressionError> {
        let cond = self.parse_or()?;
        if self.eat_single(b'?') {
            let then_val = self.parse_ternary()?;
            self.expect(b':')?;
            let else_val = self.parse_ternary()?;
            return Ok(if truthy(&cond) { then_val } else { else_val });
        }
        Ok(cond)
    }

    // or := and ('||' and)*
    fn parse_or(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_and()?;
        loop {
            if self.eat_op(b'|', b'|').is_some() {
                let rhs = self.parse_and()?;
                value = Value::Bool(truthy(&value) || truthy(&rhs));
            } else {
                return Ok(value);
            }
        }
    }

    // and := equality ('&&' equality)*
    fn parse_and(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_equality()?;
        loop {
            if self.eat_op(b'&', b'&').is_some() {
                let rhs = self.parse_equality()?;
                value = Value::Bool(truthy(&value) && truthy(&rhs));
            } else {
                return Ok(value);
            }
        }
    }

    // equality := relational (('==' | '!=') relational)*
    fn parse_equality(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_relational()?;
        loop {
            if self.eat_op(b'=', b'=').is_some() {
                let rhs = self.parse_relational()?;
                value = Value::Bool(equal_values(&value, &rhs));
            } else if self.eat_op(b'!', b'=').is_some() {
                let rhs = self.parse_relational()?;
                value = Value::Bool(!equal_values(&value, &rhs));
            } else {
                return Ok(value);
            }
        }
    }

    // relational := additive (('<' | '<=' | '>' | '>=') additive)*
    fn parse_relational(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_additive()?;
        loop {
            if self.eat_single(b'<') {
                if self.eat_single(b'=') {
                    let rhs = self.parse_additive()?;
                    value = Value::Bool(
                        compare_values(&value, &rhs)?
                            .map(|o| o <= Ordering::Equal)
                            .unwrap_or(false),
                    );
                } else {
                    let rhs = self.parse_additive()?;
                    value = Value::Bool(
                        compare_values(&value, &rhs)?
                            .map(|o| o == Ordering::Less)
                            .unwrap_or(false),
                    );
                }
            } else if self.eat_single(b'>') {
                if self.eat_single(b'=') {
                    let rhs = self.parse_additive()?;
                    value = Value::Bool(
                        compare_values(&value, &rhs)?
                            .map(|o| o >= Ordering::Equal)
                            .unwrap_or(false),
                    );
                } else {
                    let rhs = self.parse_additive()?;
                    value = Value::Bool(
                        compare_values(&value, &rhs)?
                            .map(|o| o == Ordering::Greater)
                            .unwrap_or(false),
                    );
                }
            } else {
                return Ok(value);
            }
        }
    }

    // additive := multiplicative (('+' | '-') multiplicative)*
    fn parse_additive(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_multiplicative()?;
        loop {
            if self.eat_single(b'+') {
                let rhs = self.parse_multiplicative()?;
                value = add_values(&value, &rhs)?;
            } else if self.eat_single(b'-') {
                let rhs = self.parse_multiplicative()?;
                value = sub_values(&value, &rhs)?;
            } else {
                return Ok(value);
            }
        }
    }

    // multiplicative := unary (('*' | '/' | '%') unary)*
    fn parse_multiplicative(&mut self) -> Result<Value, ExpressionError> {
        let mut value = self.parse_unary()?;
        loop {
            if self.eat_single(b'*') {
                let rhs = self.parse_unary()?;
                value = mul_values(&value, &rhs)?;
            } else if self.eat_single(b'/') {
                let rhs = self.parse_unary()?;
                value = div_values(&value, &rhs)?;
            } else if self.eat_single(b'%') {
                let rhs = self.parse_unary()?;
                value = rem_values(&value, &rhs)?;
            } else {
                return Ok(value);
            }
        }
    }

    // unary := ('!' | '-') unary | primary
    fn parse_unary(&mut self) -> Result<Value, ExpressionError> {
        if self.eat_single(b'!') {
            let value = self.parse_unary()?;
            return Ok(Value::Bool(!truthy(&value)));
        }
        if self.eat_single(b'-') {
            let value = self.parse_unary()?;
            return negate(&value);
        }
        self.parse_primary()
    }

    // primary := number | string | bool | variable-ref | identifier | '(' ternary ')'
    fn parse_primary(&mut self) -> Result<Value, ExpressionError> {
        self.skip_whitespace();
        let start = self.pos;
        let Some(b) = self.peek() else {
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unexpected end of expression",
            ));
        };

        if b.is_ascii_digit() || b == b'.' {
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
                self.pos += 1;
            }
            let text = std::str::from_utf8(&self.input[start..self.pos]).unwrap_or_default();
            if let Ok(num) = text.parse::<i64>() {
                return Ok(Value::Number(num.into()));
            }
            if let Ok(f) = text.parse::<f64>() {
                if let Some(n) = serde_json::Number::from_f64(f) {
                    return Ok(Value::Number(n));
                }
            }
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                format!("invalid number '{}'", text),
            ));
        }

        if b == b'"' || b == b'\'' {
            let quote = b;
            self.pos += 1;
            let mut text = String::new();
            while let Some(c) = self.peek() {
                if c == quote {
                    self.pos += 1;
                    return Ok(Value::String(text));
                }
                if c == b'\\' && self.input.get(self.pos + 1) == Some(&quote) {
                    text.push(quote as char);
                    self.pos += 2;
                } else {
                    text.push(c as char);
                    self.pos += 1;
                }
            }
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unterminated string literal",
            ));
        }

        if b == b'(' {
            self.pos += 1;
            let value = self.parse_ternary()?;
            self.expect(b')')?;
            return Ok(value);
        }

        if b == b'$' && self.input.get(self.pos + 1) == Some(&b'{') {
            self.pos += 2;
            let mut name = String::new();
            while let Some(c) = self.peek() {
                if c == b'}' {
                    self.pos += 1;
                    return VariableResolver::lookup_variable(&name, self.variables).ok_or_else(
                        || {
                            ExpressionError::new(
                                String::from_utf8_lossy(self.input),
                                format!("variable '{}' not found", name),
                            )
                        },
                    );
                }
                name.push(c as char);
                self.pos += 1;
            }
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unterminated variable reference",
            ));
        }

        // Identifier / keyword: true / false / null / variable name.
        if b.is_ascii_alphabetic() || b == b'_' {
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
            {
                self.pos += 1;
            }
            let name = std::str::from_utf8(&self.input[start..self.pos]).unwrap_or_default();
            match name {
                "true" => return Ok(Value::Bool(true)),
                "false" => return Ok(Value::Bool(false)),
                "null" => return Ok(Value::Null),
                _ => {
                    return VariableResolver::lookup_variable(name, self.variables).ok_or_else(
                        || {
                            ExpressionError::new(
                                String::from_utf8_lossy(self.input),
                                format!("variable '{}' not found", name),
                            )
                        },
                    );
                }
            }
        }

        Err(ExpressionError::new(
            String::from_utf8_lossy(self.input),
            format!("unexpected character '{}'", b as char),
        ))
    }
}

use std::cmp::Ordering;

/// Truthiness for boolean contexts (null/false/0/empty are falsy).
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Loose equality: numbers compare by numeric value, booleans/strings by
/// value, others by JSON equality.
pub fn equal_values(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn compare_values(a: &Value, b: &Value) -> Result<Option<Ordering>, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => Ok(x.as_f64().partial_cmp(&y.as_f64())),
        (Value::String(x), Value::String(y)) => Ok(Some(x.cmp(y))),
        _ => Ok(None),
    }
}

/// Apply an integer-aware binary numeric operation: when both operands are
/// integers and the op is `+ - * / %`, integer arithmetic is used so results
/// stay integral (e.g. `10 / 4 == 2`); otherwise float arithmetic is used.
fn number_op<F, G>(x: &serde_json::Number, y: &serde_json::Number, int_op: F, float_op: G) -> Value
where
    F: Fn(i64, i64) -> Option<i64>,
    G: Fn(f64, f64) -> Option<f64>,
{
    match (x.as_i64(), y.as_i64()) {
        (Some(a), Some(b)) => match int_op(a, b) {
            Some(i) => Value::Number(i.into()),
            None => Value::Null,
        },
        _ => match (x.as_f64(), y.as_f64()) {
            (Some(a), Some(b)) => float_op(a, b)
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            _ => Value::Null,
        },
    }
}

fn add_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            Ok(number_op(x, y, |a, b| a.checked_add(b), |a, b| Some(a + b)))
        }
        (Value::String(x), Value::String(y)) => {
            let mut joined = x.clone();
            joined.push_str(y);
            Ok(Value::String(joined))
        }
        (Value::String(x), other) => Ok(Value::String(format!("{}{}", x, other))),
        (other, Value::String(y)) => Ok(Value::String(format!("{}{}", other, y))),
        (Value::Array(x), Value::Array(y)) => {
            let mut joined = x.clone();
            joined.extend(y.iter().cloned());
            Ok(Value::Array(joined))
        }
        _ => Err(ExpressionError::new(
            format!("{} + {}", a, b),
            "operator '+' not supported for these operands",
        )),
    }
}

fn sub_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            Ok(number_op(x, y, |a, b| a.checked_sub(b), |a, b| Some(a - b)))
        }
        _ => Err(ExpressionError::new(
            format!("{} - {}", a, b),
            "operator '-' requires numeric operands",
        )),
    }
}

fn mul_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            Ok(number_op(x, y, |a, b| a.checked_mul(b), |a, b| Some(a * b)))
        }
        _ => Err(ExpressionError::new(
            format!("{} * {}", a, b),
            "operator '*' requires numeric operands",
        )),
    }
}

fn div_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if y.as_f64().unwrap_or(0.0) == 0.0 {
                return Err(ExpressionError::new(
                    format!("{} / {}", a, b),
                    "division by zero",
                ));
            }
            Ok(number_op(x, y, |a, b| a.checked_div(b), |a, b| Some(a / b)))
        }
        _ => Err(ExpressionError::new(
            format!("{} / {}", a, b),
            "operator '/' requires numeric operands",
        )),
    }
}

fn rem_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if y.as_f64().unwrap_or(0.0) == 0.0 {
                return Err(ExpressionError::new(
                    format!("{} % {}", a, b),
                    "modulo by zero",
                ));
            }
            Ok(number_op(x, y, |a, b| a.checked_rem(b), |a, b| Some(a % b)))
        }
        _ => Err(ExpressionError::new(
            format!("{} % {}", a, b),
            "operator '%' requires numeric operands",
        )),
    }
}

fn negate(a: &Value) -> Result<Value, ExpressionError> {
    match a {
        Value::Number(x) => match x.as_i64() {
            Some(i) => Ok(Value::Number(i.checked_neg().unwrap_or(0).into())),
            None => {
                let neg = -x.as_f64().unwrap_or(0.0);
                Ok(serde_json::Number::from_f64(neg)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            }
        },
        _ => Err(ExpressionError::new(
            a.to_string(),
            "unary '-' requires a numeric operand",
        )),
    }
}

pub struct VariableResolver;

impl VariableResolver {
    pub fn resolve(input: &Value, variables: &VariableStore) -> Value {
        match input {
            Value::String(s) => Self::resolve_str(s, variables),
            Value::Object(map) => {
                let resolved: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::resolve(v, variables)))
                    .collect();
                Value::Object(resolved)
            }
            Value::Array(arr) => {
                let resolved: Vec<Value> =
                    arr.iter().map(|v| Self::resolve(v, variables)).collect();
                Value::Array(resolved)
            }
            other => other.clone(),
        }
    }

    pub fn resolve_str(input: &str, variables: &VariableStore) -> Value {
        if input.starts_with("${") && input.ends_with("}") {
            let var_name = &input[2..input.len() - 1];
            if let Some(v) = Self::lookup_variable(var_name, variables) {
                return v;
            }
        }

        let mut result = input.to_string();
        let mut start = 0;

        while let Some(pos) = result[start..].find("${") {
            let abs_pos = start + pos;
            if let Some(end) = result[abs_pos..].find('}') {
                let abs_end = abs_pos + end;
                let var_name = &result[abs_pos + 2..abs_end];
                if let Some(v) = Self::lookup_variable(var_name, variables) {
                    let replacement = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    result.replace_range(abs_pos..=abs_end, &replacement);
                    start = abs_pos + replacement.len();
                } else {
                    start = abs_end + 1;
                }
            } else {
                break;
            }
        }

        Value::String(result)
    }

    fn lookup_variable(path: &str, variables: &VariableStore) -> Option<Value> {
        // `variables.x` is a scope-prefixed path: it reads the flat variable
        // `x` from the store, not a nested key
        // under a literal "variables" entry.
        let path = path.strip_prefix("variables.").unwrap_or(path);
        let parts: Vec<&str> = path.split('.').collect();
        let first = parts.first()?;

        let mut current = variables.get(*first)?.clone();

        for part in &parts[1..] {
            if let Value::Object(map) = &current {
                current = serde_json::from_value(serde_json::to_value(map).ok()?).ok()?;
                if let Value::Object(map) = &current {
                    current = map.get(*part)?.clone();
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }

        Some(current)
    }
}

/// Evaluate an expression string against the variable store. Falls back to
/// plain variable interpolation (the legacy `VariableResolver` behaviour)
/// when the expression contains no expression operators, so existing
/// `${var}` configs keep working.
pub fn evaluate_expression(
    expression: &str,
    variables: &VariableStore,
) -> Result<Value, ExpressionError> {
    let trimmed = expression.trim();
    if !trimmed.is_empty() && looks_like_expression(trimmed) {
        match ExprEvaluator::new(trimmed, variables).evaluate() {
            Ok(value) => Ok(value),
            // A template literal (a string embedding `${...}` refs) may
            // legitimately contain operator characters in its literal text
            // (e.g. `${variables.step1}-done`). When the expression parse
            // fails, fall back to plain interpolation for such templates.
            Err(_e) if trimmed.contains("${") => Ok(VariableResolver::resolve(
                &Value::String(expression.to_string()),
                variables,
            )),
            Err(e) => Err(e),
        }
    } else {
        Ok(VariableResolver::resolve(
            &Value::String(expression.to_string()),
            variables,
        ))
    }
}

/// Whether a string contains expression operators beyond a bare variable
/// reference / literal (arithmetic, comparison, logic, ternary, grouping).
fn looks_like_expression(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '"' || c == '\'' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => in_string = true,
            '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!' | '&' | '|' | '?' | ':' | '('
            | ')' => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Convert a value to the declared `variableType` (number/string/boolean/
/// array/object). Conversion failures surface as explicit errors rather than
/// silent coercion.
pub fn convert_variable_type(
    variable_name: &str,
    value: Value,
    variable_type: Option<&str>,
) -> Result<Value, crate::error::WorkflowError> {
    let Some(r#type) = variable_type else {
        return Ok(value);
    };
    let converted = match (r#type, &value) {
        ("number", Value::Number(n)) => Value::Number(n.clone()),
        ("number", Value::String(s)) => {
            let trimmed = s.trim();
            if let Ok(i) = trimmed.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = trimmed.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| {
                        crate::error::WorkflowError::VariableError(format!(
                            "Cannot convert value '{}' of variable '{}' to number",
                            s, variable_name
                        ))
                    })?
            } else {
                return Err(crate::error::WorkflowError::VariableError(format!(
                    "Cannot convert value '{}' of variable '{}' to number",
                    s, variable_name
                )));
            }
        }
        ("string", Value::String(s)) => Value::String(s.clone()),
        ("string", Value::Null) => Value::String(String::new()),
        ("string", other) => Value::String(other.to_string()),
        ("boolean", Value::Bool(b)) => Value::Bool(*b),
        ("boolean", Value::String(s)) => match s.trim().to_lowercase().as_str() {
            "true" | "1" => Value::Bool(true),
            "false" | "0" | "" => Value::Bool(false),
            other => {
                return Err(crate::error::WorkflowError::VariableError(format!(
                    "Cannot convert value '{}' of variable '{}' to boolean",
                    other, variable_name
                )))
            }
        },
        ("boolean", Value::Number(n)) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
        ("array", Value::Array(a)) => Value::Array(a.clone()),
        ("array", Value::String(s)) => Value::Array(
            s.split(',')
                .map(|part| Value::String(part.trim().to_string()))
                .collect(),
        ),
        ("object", Value::Object(o)) => Value::Object(o.clone()),
        ("object", Value::String(s)) => serde_json::from_str::<serde_json::Map<String, Value>>(s)
            .map(Value::Object)
            .map_err(|_| {
                crate::error::WorkflowError::VariableError(format!(
                    "Cannot convert value '{}' of variable '{}' to object",
                    s, variable_name
                ))
            })?,
        (other, _) => {
            return Err(crate::error::WorkflowError::VariableError(format!(
                "Unsupported variable type '{}' for variable '{}'",
                other, variable_name
            )))
        }
    };
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(vars: &[(&str, Value)]) -> VariableStore {
        let s = create_variable_store();
        for (k, v) in vars {
            s.insert(k.to_string(), v.clone());
        }
        s
    }

    #[test]
    fn test_resolve_simple() {
        let vars = store(&[("name", Value::String("world".to_string()))]);
        let result = VariableResolver::resolve_str("hello ${name}", &vars);
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_resolve_missing() {
        let vars = store(&[]);
        let result = VariableResolver::resolve_str("hello ${missing}", &vars);
        assert_eq!(result, Value::String("hello ${missing}".to_string()));
    }

    #[test]
    fn test_resolve_multiple() {
        let vars = store(&[
            ("a", Value::String("A".to_string())),
            ("b", Value::String("B".to_string())),
        ]);
        let result = VariableResolver::resolve_str("${a} and ${b}", &vars);
        assert_eq!(result, Value::String("A and B".to_string()));
    }

    #[test]
    fn expression_arithmetic_and_comparison() {
        let vars = store(&[("a", Value::from(10)), ("b", Value::from(3))]);
        assert_eq!(
            evaluate_expression("a + b * 2", &vars).unwrap(),
            Value::from(16)
        );
        assert_eq!(
            evaluate_expression("(a + b) * 2", &vars).unwrap(),
            Value::from(26)
        );
        assert_eq!(evaluate_expression("a % b", &vars).unwrap(), Value::from(1));
        assert_eq!(
            evaluate_expression("a > b", &vars).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("a >= 10 && b < 4", &vars).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("a == 10", &vars).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn expression_string_concat_and_ternary() {
        let vars = store(&[
            ("name", Value::String("world".to_string())),
            ("n", Value::from(5)),
        ]);
        assert_eq!(
            evaluate_expression("'hello ' + name", &vars).unwrap(),
            Value::String("hello world".to_string())
        );
        assert_eq!(
            evaluate_expression("n > 3 ? 'big' : 'small'", &vars).unwrap(),
            Value::String("big".to_string())
        );
        assert_eq!(
            evaluate_expression("!true", &vars).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            evaluate_expression("-n + 2", &vars).unwrap(),
            Value::from(-3)
        );
    }

    #[test]
    fn expression_variable_reference_and_dotted_path() {
        let vars = store(&[(
            "user",
            Value::Object(serde_json::Map::from_iter([(
                "age".to_string(),
                Value::from(30),
            )])),
        )]);
        assert_eq!(
            evaluate_expression("user.age >= 18", &vars).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("${user.age}", &vars).unwrap(),
            Value::from(30)
        );
    }

    #[test]
    fn expression_errors_are_explicit() {
        let vars = store(&[("n", Value::from(10))]);
        assert!(evaluate_expression("n / 0", &vars).is_err());
        assert!(evaluate_expression("missing + 1", &vars).is_err());
        assert!(evaluate_expression("'abc' - 1", &vars).is_err());
    }

    #[test]
    fn variables_scope_prefix_resolves_flat_store() {
        let vars = store(&[("step1", Value::String("hi".to_string()))]);
        assert_eq!(
            evaluate_expression("${variables.step1}-done", &vars).unwrap(),
            Value::String("hi-done".to_string())
        );
        assert_eq!(
            evaluate_expression("variables.step1 == 'hi'", &vars).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn legacy_interpolation_falls_back() {
        let vars = store(&[("greeting", Value::String("hi".to_string()))]);
        assert_eq!(
            evaluate_expression("hello ${greeting}", &vars).unwrap(),
            Value::String("hello hi".to_string())
        );
        // A bare reference without operators stays on the interpolation path.
        assert_eq!(
            evaluate_expression("${greeting}", &vars).unwrap(),
            Value::String("hi".to_string())
        );
    }

    #[test]
    fn type_conversion_roundtrip() {
        use crate::error::WorkflowError;
        assert_eq!(
            convert_variable_type("v", Value::String("42".to_string()), Some("number")).unwrap(),
            Value::from(42)
        );
        assert_eq!(
            convert_variable_type("v", Value::from(1), Some("string")).unwrap(),
            Value::String("1".to_string())
        );
        assert_eq!(
            convert_variable_type("v", Value::String("true".to_string()), Some("boolean")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            convert_variable_type("v", Value::String("a,b,c".to_string()), Some("array")).unwrap(),
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );
        let err = convert_variable_type(
            "v",
            Value::String("not-a-number".to_string()),
            Some("number"),
        )
        .unwrap_err();
        assert!(matches!(err, WorkflowError::VariableError(_)));
        let err = convert_variable_type("v", Value::String("nope".to_string()), Some("boolean"))
            .unwrap_err();
        assert!(matches!(err, WorkflowError::VariableError(_)));
    }
}
