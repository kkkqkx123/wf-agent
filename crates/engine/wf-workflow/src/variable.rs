//! Workflow assembly-layer variables: the `${...}` syntax.
//!
//! Dollar-brace references belong to variable transport (moving values
//! between nodes, plus expression evaluation in `ExprEvaluator`), while
//! double-brace `{{...}}` placeholders belong to the text render layer
//! (`wf_common::template` via the resource template engine). The two layers
//! run in order on purpose: `${...}` first, then `{{...}}`. A template that
//! legitimately carries both (for example script node templates) is not a
//! syntax mix-up, so no field-level cross-syntax rejection exists.
//!
//! Field ownership cheatsheet: user-interaction `prompt` fields resolve
//! here (`${...}`); message `content_template` fields render later through
//! the `{{input}}` substitution.

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use wf_execution_shared::context::{resolve_system_path, SystemContext};

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
    system: Option<&'a SystemContext>,
}

impl<'a> ExprEvaluator<'a> {
    pub fn new(
        input: &'a str,
        variables: &'a VariableStore,
        system: Option<&'a SystemContext>,
    ) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            variables,
            system,
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
            return self.resolve_braced_span();
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
                    return VariableResolver::lookup_variable(name, self.variables, self.system)
                        .ok_or_else(|| {
                            ExpressionError::new(
                                String::from_utf8_lossy(self.input),
                                format!("variable '{}' not found", name),
                            )
                        });
                }
            }
        }

        Err(ExpressionError::new(
            String::from_utf8_lossy(self.input),
            format!("unexpected character '{}'", b as char),
        ))
    }

    fn resolve_braced_span(&mut self) -> Result<Value, ExpressionError> {
        let rest = std::str::from_utf8(&self.input[self.pos..]).unwrap_or_default();
        let mut spans = wf_common::template::scan_variable_spans(rest);
        if spans.is_empty() || spans[0].start != 0 {
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unterminated variable reference",
            ));
        }
        let span = spans.remove(0);
        if !span.closed {
            return Err(ExpressionError::new(
                String::from_utf8_lossy(self.input),
                "unterminated variable reference",
            ));
        }
        let name = span.path;
        self.pos += span.end;
        VariableResolver::lookup_variable(&name, self.variables, self.system).ok_or_else(|| {
            ExpressionError::new(
                String::from_utf8_lossy(self.input),
                format!("variable '{}' not found", name),
            )
        })
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
/// Overflow, division by zero remnants and non-finite float results are
/// explicit errors so misconfiguration never degrades to a silent null.
fn number_op<F, G>(
    expr: &str,
    x: &serde_json::Number,
    y: &serde_json::Number,
    int_op: F,
    float_op: G,
) -> Result<Value, ExpressionError>
where
    F: Fn(i64, i64) -> Option<i64>,
    G: Fn(f64, f64) -> Option<f64>,
{
    match (x.as_i64(), y.as_i64()) {
        (Some(a), Some(b)) => match int_op(a, b) {
            Some(i) => Ok(Value::Number(i.into())),
            None => Err(ExpressionError::new(
                expr.to_string(),
                format!("numeric operation overflowed for operands {x} and {y}"),
            )),
        },
        _ => match (x.as_f64(), y.as_f64()) {
            (Some(a), Some(b)) => match float_op(a, b) {
                Some(v) => serde_json::Number::from_f64(v).map(Value::Number).ok_or_else(|| {
                    ExpressionError::new(
                        expr.to_string(),
                        format!("numeric operation produced a non-finite value for operands {x} and {y}"),
                    )
                }),
                None => Err(ExpressionError::new(
                    expr.to_string(),
                    format!("numeric operation failed for operands {x} and {y}"),
                )),
            },
            _ => Err(ExpressionError::new(
                expr.to_string(),
                "numeric operation requires numeric operands",
            )),
        },
    }
}

fn add_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => number_op(
            &format!("{a} + {b}"),
            x,
            y,
            |a, b| a.checked_add(b),
            |a, b| Some(a + b),
        ),
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
        (Value::Number(x), Value::Number(y)) => number_op(
            &format!("{a} - {b}"),
            x,
            y,
            |a, b| a.checked_sub(b),
            |a, b| Some(a - b),
        ),
        _ => Err(ExpressionError::new(
            format!("{} - {}", a, b),
            "operator '-' requires numeric operands",
        )),
    }
}

fn mul_values(a: &Value, b: &Value) -> Result<Value, ExpressionError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => number_op(
            &format!("{a} * {b}"),
            x,
            y,
            |a, b| a.checked_mul(b),
            |a, b| Some(a * b),
        ),
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
            number_op(
                &format!("{a} / {b}"),
                x,
                y,
                |a, b| a.checked_div(b),
                |a, b| Some(a / b),
            )
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
            number_op(
                &format!("{a} % {b}"),
                x,
                y,
                |a, b| a.checked_rem(b),
                |a, b| Some(a % b),
            )
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
            Some(i) => i.checked_neg().map(|v| Value::Number(v.into())).ok_or_else(|| {
                ExpressionError::new(a.to_string(), "unary '-' overflowed")
            }),
            None => {
                let neg = -x.as_f64().unwrap_or(0.0);
                serde_json::Number::from_f64(neg).map(Value::Number).ok_or_else(|| {
                    ExpressionError::new(a.to_string(), "unary '-' produced a non-finite value")
                })
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
    /// Assembly-layer resolution for `${path}` references.
    /// A whole-string single reference preserves the value type;
    /// embedded references coerce with display semantics for text transport.
    /// Condition literals and prompt display each own their converters.
    /// The optional system view resolves read-only `execution.*` and
    /// `node.*` paths without touching the variable map.
    pub fn resolve(
        input: &Value,
        variables: &VariableStore,
        system: Option<&SystemContext>,
    ) -> Value {
        match input {
            Value::String(s) => Self::resolve_str(s, variables, system),
            Value::Object(map) => {
                let resolved: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::resolve(v, variables, system)))
                    .collect();
                Value::Object(resolved)
            }
            Value::Array(arr) => {
                let resolved: Vec<Value> = arr
                    .iter()
                    .map(|v| Self::resolve(v, variables, system))
                    .collect();
                Value::Array(resolved)
            }
            other => other.clone(),
        }
    }

    pub fn resolve_str(
        input: &str,
        variables: &VariableStore,
        system: Option<&SystemContext>,
    ) -> Value {
        let spans = wf_common::template::scan_variable_spans(input);
        if spans.len() == 1
            && spans[0].closed
            && spans[0].start == 0
            && spans[0].end == input.len()
            && !spans[0].path.is_empty()
        {
            if let Some(v) = Self::lookup_variable(&spans[0].path, variables, system) {
                return v;
            }
        }

        if spans.is_empty() {
            return Value::String(input.to_string());
        }
        let mut result = String::with_capacity(input.len());
        let mut cursor = 0;
        for span in spans {
            result.push_str(&input[cursor..span.start]);
            if !span.closed {
                result.push_str(&input[span.start..]);
                cursor = input.len();
                break;
            }
            if span.path.is_empty() {
                result.push_str(&input[span.start..span.end]);
            } else if let Some(v) = Self::lookup_variable(&span.path, variables, system) {
                result.push_str(&wf_common::template::value_to_display_string(&v));
            } else {
                tracing::debug!(
                    variable = span.path.as_str(),
                    "assembly variable missing; leaving reference verbatim"
                );
                result.push_str(&input[span.start..span.end]);
            }
            cursor = span.end;
        }
        result.push_str(&input[cursor..]);

        Value::String(result)
    }

    fn lookup_variable(
        path: &str,
        variables: &VariableStore,
        system: Option<&SystemContext>,
    ) -> Option<Value> {
        if let Some(ctx) = system {
            if let Some(value) = resolve_system_path(ctx, path) {
                return Some(value);
            }
        }
        // `variables.x` is a scope-prefixed path: it reads the flat variable
        // `x` from the store, not a nested key
        // under a literal "variables" entry.
        let path = path.strip_prefix("variables.").unwrap_or(path);
        if let Some(ctx) = system {
            if let Some(value) = resolve_system_path(ctx, path) {
                return Some(value);
            }
        }
        if wf_common::template::validate_template_path(path).is_some() {
            return None;
        }
        let mut parts = path.split('.');
        let first = parts.next()?;
        let mut current = variables.get(first)?.clone();

        for part in parts {
            match &current {
                Value::Object(map) => {
                    current = map.get(part)?.clone();
                }
                Value::Array(items) => {
                    let index: usize = part.parse().ok()?;
                    current = items.get(index)?.clone();
                }
                _ => return None,
            }
        }

        Some(current)
    }
}

/// Evaluate an expression string against the variable store. Falls back to
/// plain variable interpolation when the expression contains no expression
/// operators, so `${var}` configs keep working. Template literals that
/// embed `${...}` refs fall back to interpolation only for syntax-shape
/// failures; semantic failures (missing variable, type mismatch, division
/// by zero, overflow) are returned as errors instead of masked strings.
pub fn evaluate_expression(
    expression: &str,
    variables: &VariableStore,
    system: Option<&SystemContext>,
) -> Result<Value, ExpressionError> {
    let trimmed = expression.trim();
    if !trimmed.is_empty() && looks_like_expression(trimmed) {
        match ExprEvaluator::new(trimmed, variables, system).evaluate() {
            Ok(value) => Ok(value),
            Err(e) if trimmed.contains("${") && !is_semantic_evaluation_error(&e) => {
                tracing::debug!(
                    expression = trimmed,
                    error = %e,
                    "expression parse failed on a template literal; degraded to plain interpolation"
                );
                Ok(VariableResolver::resolve(
                    &Value::String(expression.to_string()),
                    variables,
                    system,
                ))
            }
            Err(e) => Err(e),
        }
    } else {
        Ok(VariableResolver::resolve(
            &Value::String(expression.to_string()),
            variables,
            system,
        ))
    }
}

/// Whether an evaluation failure reflects a semantic problem that must
/// surface instead of degrading to an interpolated string.
fn is_semantic_evaluation_error(e: &ExpressionError) -> bool {
    let msg = e.message.as_str();
    msg.contains("not found")
        || msg.contains("requires")
        || msg.contains("division by zero")
        || msg.contains("modulo by zero")
        || msg.contains("overflowed")
        || msg.contains("non-finite")
        || msg.contains("failed for operands")
}

/// Whether a string contains expression operators beyond a bare variable
/// reference / literal (arithmetic, comparison, logic, ternary, grouping).
/// Operators inside `${...}` spans never count, and an operator directly
/// adjacent to a span edge (`${a}-done`) counts as template text, not as an
/// expression operator. Spaced or multi-character operators still count.
fn looks_like_expression(s: &str) -> bool {
    let spans = wf_common::template::scan_variable_spans(s);
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if spans.iter().any(|sp| i >= sp.start && i < sp.end) {
            i += 1;
            continue;
        }
        if let Some(quote) = in_string {
            if c == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_string = Some(c);
            i += 1;
            continue;
        }
        let is_operator = matches!(
            c,
            b'+' | b'-' | b'*' | b'/' | b'%' | b'<' | b'>' | b'=' | b'!' | b'&' | b'|'
                | b'?' | b':' | b'(' | b')'
        );
        if is_operator {
            let touches_span = spans
                .iter()
                .any(|sp| sp.closed && (i == sp.end || (i + 1) == sp.start));
            if !touches_span {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Convert a value to the declared `variableType` (number/string/boolean/
/// array/object). Conversion failures surface as explicit errors rather than
/// silent coercion. Empty text never stands in for a missing value: whether
/// empty is allowed is expressed by the caller through presence and defaults,
/// not by coercing empty into a typed value here.
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
            "false" | "0" => Value::Bool(false),
            other => {
                return Err(crate::error::WorkflowError::VariableError(format!(
                    "Cannot convert value '{}' of variable '{}' to boolean",
                    other, variable_name
                )))
            }
        },
        ("boolean", Value::Number(n)) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
        ("array", Value::Array(a)) => Value::Array(a.clone()),
        ("array", Value::String(s)) if s.trim().is_empty() => Value::Array(Vec::new()),
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
        let result = VariableResolver::resolve_str("hello ${name}", &vars, None);
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_resolve_missing() {
        let vars = store(&[]);
        let result = VariableResolver::resolve_str("hello ${missing}", &vars, None);
        assert_eq!(result, Value::String("hello ${missing}".to_string()));
    }

    #[test]
    fn test_resolve_multiple() {
        let vars = store(&[
            ("a", Value::String("A".to_string())),
            ("b", Value::String("B".to_string())),
        ]);
        let result = VariableResolver::resolve_str("${a} and ${b}", &vars, None);
        assert_eq!(result, Value::String("A and B".to_string()));
    }

    #[test]
    fn expression_arithmetic_and_comparison() {
        let vars = store(&[("a", Value::from(10)), ("b", Value::from(3))]);
        assert_eq!(
            evaluate_expression("a + b * 2", &vars, None).unwrap(),
            Value::from(16)
        );
        assert_eq!(
            evaluate_expression("(a + b) * 2", &vars, None).unwrap(),
            Value::from(26)
        );
        assert_eq!(evaluate_expression("a % b", &vars, None).unwrap(), Value::from(1));
        assert_eq!(
            evaluate_expression("a > b", &vars, None).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("a >= 10 && b < 4", &vars, None).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("a == 10", &vars, None).unwrap(),
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
            evaluate_expression("'hello ' + name", &vars, None).unwrap(),
            Value::String("hello world".to_string())
        );
        assert_eq!(
            evaluate_expression("n > 3 ? 'big' : 'small'", &vars, None).unwrap(),
            Value::String("big".to_string())
        );
        assert_eq!(
            evaluate_expression("!true", &vars, None).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            evaluate_expression("-n + 2", &vars, None).unwrap(),
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
            evaluate_expression("user.age >= 18", &vars, None).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate_expression("${user.age}", &vars, None).unwrap(),
            Value::from(30)
        );
    }

    #[test]
    fn expression_errors_are_explicit() {
        let vars = store(&[("n", Value::from(10))]);
        assert!(evaluate_expression("n / 0", &vars, None).is_err());
        assert!(evaluate_expression("missing + 1", &vars, None).is_err());
        assert!(evaluate_expression("'abc' - 1", &vars, None).is_err());
    }

    #[test]
    fn variables_scope_prefix_resolves_flat_store() {
        let vars = store(&[("step1", Value::String("hi".to_string()))]);
        assert_eq!(
            evaluate_expression("${variables.step1}-done", &vars, None).unwrap(),
            Value::String("hi-done".to_string())
        );
        assert_eq!(
            evaluate_expression("variables.step1 == 'hi'", &vars, None).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn array_index_path_resolves() {
        let vars = store(&[("items", serde_json::json!(["a", "b"]))]);
        assert_eq!(
            VariableResolver::resolve_str("${items.1}", &vars, None),
            Value::String("b".to_string())
        );
    }

    #[test]
    fn legacy_interpolation_falls_back() {
        let vars = store(&[("greeting", Value::String("hi".to_string()))]);
        assert_eq!(
            evaluate_expression("hello ${greeting}", &vars, None).unwrap(),
            Value::String("hello hi".to_string())
        );
        // A bare reference without operators stays on the interpolation path.
        assert_eq!(
            evaluate_expression("${greeting}", &vars, None).unwrap(),
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
        let err =
            convert_variable_type("v", Value::String(String::new()), Some("boolean")).unwrap_err();
        assert!(matches!(err, WorkflowError::VariableError(_)));
        assert_eq!(
            convert_variable_type("v", Value::String(String::new()), Some("array")).unwrap(),
            Value::Array(Vec::new())
        );
    }

    #[test]
    fn overflow_is_explicit_not_null() {
        let vars = store(&[("big", Value::from(i64::MAX))]);
        assert!(evaluate_expression("big + 1", &vars, None).is_err());
        assert!(evaluate_expression("big * 2", &vars, None).is_err());
    }

    #[test]
    fn template_suffix_stays_interpolation() {
        let vars = store(&[("step1", Value::String("hi".to_string()))]);
        assert_eq!(
            evaluate_expression("${step1}-done", &vars, None).unwrap(),
            Value::String("hi-done".to_string())
        );
    }

    #[test]
    fn semantic_error_is_not_masked_by_interpolation() {
        let vars = store(&[("a", Value::from(1))]);
        assert!(evaluate_expression("${a} / 0", &vars, None).is_err());
        assert!(evaluate_expression("${a} + missing", &vars, None).is_err());
    }

    #[test]
    fn system_context_resolves_readonly_paths() {
        let vars = store(&[]);
        let system = SystemContext::new("exec-1", "n1", "VARIABLE");
        assert_eq!(
            VariableResolver::resolve_str("${execution.id}", &vars, Some(&system)),
            Value::String("exec-1".to_string())
        );
        assert_eq!(
            VariableResolver::resolve_str("${node.id}", &vars, Some(&system)),
            Value::String("n1".to_string())
        );
        assert_eq!(
            evaluate_expression("node.type == 'VARIABLE'", &vars, Some(&system)).unwrap(),
            Value::Bool(true)
        );
    }
}
