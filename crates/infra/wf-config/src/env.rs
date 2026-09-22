//! Environment variable parsing and override application.
//!
//! Provides a declarative mapping from env var names to typed values.
//! Used by the orchestrator to apply `WF_*` overrides to infrastructure
//! config.
//!
//! Error policy is deliberately fail-fast here: an invalid `WF_*` value
//! aborts assembly with `ConfigError::EnvVar` instead of silently keeping
//! the old value, because these numbers feed execution guards (timeouts,
//! iteration caps) where a typoed override running unnoticed is worse than
//! a loud boot failure. The server middleware overlay (`wf-server`) uses
//! the opposite policy (warn and keep) because there availability wins:
//! a typo must never take the listener down. Both policies share the
//! parsers below so `true/1/yes` spellings stay identical everywhere.
//!
//! File-level interpolation (`${VAR}` / `${VAR:default}`) is the single
//! env expansion applied to file content before parsing. Only uppercase
//! names match so workflow expressions (`${input.a}`) stay untouched.
//! `WF_*` typed overrides apply at assembly time on top of interpolated
//! files. `{{parameters.*}}` substitution lives at workflow execution time.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::error::{ConfigError, ConfigResult};

pub type EnvParser = Box<dyn Fn(&str) -> ConfigResult<EnvValue> + Send + Sync>;

#[derive(Debug, Clone)]
pub enum EnvValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<String>),
}

impl EnvValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            EnvValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            EnvValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            EnvValue::Int(i) => Some(*i),
            _ => None,
        }
    }
}

pub fn env_parse_string(value: &str) -> ConfigResult<EnvValue> {
    Ok(EnvValue::String(value.to_string()))
}

pub fn env_parse_int(value: &str) -> ConfigResult<EnvValue> {
    value
        .parse::<i64>()
        .map(EnvValue::Int)
        .map_err(|e| ConfigError::EnvVar(format!("failed to parse int: {e}")))
}

pub fn env_parse_float(value: &str) -> ConfigResult<EnvValue> {
    value
        .parse::<f64>()
        .map(EnvValue::Float)
        .map_err(|e| ConfigError::EnvVar(format!("failed to parse float: {e}")))
}

pub fn env_parse_bool(value: &str) -> ConfigResult<EnvValue> {
    let lower = value.to_lowercase();
    Ok(EnvValue::Bool(
        lower == "true" || lower == "1" || lower == "yes",
    ))
}

pub fn env_parse_list(value: &str) -> ConfigResult<EnvValue> {
    Ok(EnvValue::List(
        value
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    ))
}

pub fn env_parse_json<T: serde::de::DeserializeOwned>(value: &str) -> ConfigResult<T> {
    serde_json::from_str(value)
        .map_err(|e| ConfigError::EnvVar(format!("failed to parse JSON: {e}")))
}

pub fn to_env_name(prefix: &str, key: &str) -> String {
    let mut result = String::with_capacity(prefix.len() + key.len() + 1);
    result.push_str(prefix);
    for ch in key.chars() {
        if ch.is_ascii_lowercase() {
            result.push(ch.to_ascii_uppercase());
        } else if ch.is_ascii_uppercase() {
            result.push('_');
            result.push(ch);
        } else {
            result.push(ch.to_ascii_uppercase());
        }
    }
    result
}

static ENV_INTERPOLATION_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)(?::-([^}]*)|:([^}]*))?\}")
        .expect("invariant: env interpolation pattern is fixed and must compile")
});

/// Expand `${VAR}` and `${VAR:default}` placeholders in raw file content.
///
/// A set non-empty variable wins; otherwise the declared default wins;
/// without either the placeholder is left unchanged so missing secrets stay
/// visible instead of silently becoming empty strings.
pub fn expand_env_vars(content: &str) -> String {
    ENV_INTERPOLATION_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let name = &caps[1];
            let default = caps.get(2).or_else(|| caps.get(3)).map(|m| m.as_str());
            match std::env::var(name) {
                Ok(value) if !value.is_empty() => value,
                _ => default.unwrap_or(&caps[0]).to_string(),
            }
        })
        .to_string()
}

/// Uppercase `${VAR}` placeholder names present in the text in first-seen
/// order. Lowercase workflow expressions never match the expansion pattern
/// and are never reported here.
pub fn list_env_placeholders(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for caps in ENV_INTERPOLATION_REGEX.captures_iter(content) {
        let name = caps
            .get(1)
            .expect("invariant: capture group 1 is always present for a matched pattern")
            .as_str()
            .to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Fail when any `${VAR}` placeholder would survive [`expand_env_vars`].
/// Startup paths for secrets and file content call this explicitly; the
/// default expansion itself stays lenient so optional placeholders preview
/// verbatim.
pub fn ensure_no_unresolved_env_vars(content: &str) -> ConfigResult<()> {
    let missing = find_unresolved_env_vars(content);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::EnvVar(format!(
            "unresolved environment placeholders: [{}]",
            missing.join(", ")
        )))
    }
}

/// Placeholders that would survive [`expand_env_vars`]: unset without a
/// declared default. Callers use this for explicit startup checks; the
/// default expansion keeps such spans verbatim.
pub fn find_unresolved_env_vars(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for caps in ENV_INTERPOLATION_REGEX.captures_iter(content) {
        let name = &caps[1];
        let default = caps.get(2).or_else(|| caps.get(3)).map(|m| m.as_str());
        let resolved = std::env::var(name).is_ok_and(|v| !v.is_empty()) || default.is_some();
        if !resolved {
            let name = name.to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

pub struct EnvMappingEntry {
    pub env_var: String,
    pub parser: EnvParser,
    pub default: Option<EnvValue>,
    pub required: bool,
}

pub struct EnvMappingBuilder {
    entries: HashMap<String, EnvMappingEntry>,
}

impl Default for EnvMappingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvMappingBuilder {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn string(mut self, key: &str, env_var: &str, default: Option<String>) -> Self {
        self.entries.insert(
            key.to_string(),
            EnvMappingEntry {
                env_var: env_var.to_string(),
                parser: Box::new(env_parse_string),
                default: default.map(EnvValue::String),
                required: false,
            },
        );
        self
    }

    pub fn int(mut self, key: &str, env_var: &str, default: Option<i64>) -> Self {
        self.entries.insert(
            key.to_string(),
            EnvMappingEntry {
                env_var: env_var.to_string(),
                parser: Box::new(env_parse_int),
                default: default.map(EnvValue::Int),
                required: false,
            },
        );
        self
    }

    pub fn boolean(mut self, key: &str, env_var: &str, default: Option<bool>) -> Self {
        self.entries.insert(
            key.to_string(),
            EnvMappingEntry {
                env_var: env_var.to_string(),
                parser: Box::new(env_parse_bool),
                default: default.map(EnvValue::Bool),
                required: false,
            },
        );
        self
    }

    pub fn list(mut self, key: &str, env_var: &str, default: Option<Vec<String>>) -> Self {
        self.entries.insert(
            key.to_string(),
            EnvMappingEntry {
                env_var: env_var.to_string(),
                parser: Box::new(env_parse_list),
                default: default.map(EnvValue::List),
                required: false,
            },
        );
        self
    }

    pub fn custom(
        mut self,
        key: &str,
        env_var: &str,
        parser: EnvParser,
        default: Option<EnvValue>,
    ) -> Self {
        self.entries.insert(
            key.to_string(),
            EnvMappingEntry {
                env_var: env_var.to_string(),
                parser,
                default,
                required: false,
            },
        );
        self
    }

    pub fn build(self) -> HashMap<String, EnvMappingEntry> {
        self.entries
    }
}

pub fn apply_env_overrides(
    mut apply_fn: impl FnMut(&str, EnvValue),
    mapping: &HashMap<String, EnvMappingEntry>,
) -> ConfigResult<()> {
    for (key, entry) in mapping {
        match std::env::var(&entry.env_var) {
            Ok(value) => {
                if value.is_empty() {
                    continue;
                }
                let parsed = (entry.parser)(&value).map_err(|e| {
                    ConfigError::EnvVar(format!("failed to parse env var {}: {e}", entry.env_var))
                })?;
                apply_fn(key, parsed);
            }
            Err(std::env::VarError::NotPresent) => {
                if entry.required && entry.default.is_none() {
                    return Err(ConfigError::EnvVar(format!(
                        "required env var {} is not set",
                        entry.env_var
                    )));
                }
                if let Some(ref default) = entry.default {
                    apply_fn(key, default.clone());
                }
            }
            Err(e) => return Err(ConfigError::EnvVar(e.to_string())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_parse_string() {
        let result = env_parse_string("hello").unwrap();
        assert_eq!(result.as_string(), Some("hello"));
    }

    #[test]
    fn test_env_parse_int() {
        let result = env_parse_int("42").unwrap();
        assert_eq!(result.as_int(), Some(42));
        assert!(env_parse_int("not_a_number").is_err());
    }

    #[test]
    fn test_env_parse_bool() {
        assert_eq!(env_parse_bool("true").unwrap().as_bool(), Some(true));
        assert_eq!(env_parse_bool("1").unwrap().as_bool(), Some(true));
        assert_eq!(env_parse_bool("yes").unwrap().as_bool(), Some(true));
        assert_eq!(env_parse_bool("false").unwrap().as_bool(), Some(false));
        assert_eq!(env_parse_bool("0").unwrap().as_bool(), Some(false));
    }

    #[test]
    fn test_env_parse_list() {
        let result = env_parse_list("a,b,c").unwrap();
        match result {
            EnvValue::List(list) => assert_eq!(list, vec!["a", "b", "c"]),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_to_env_name() {
        assert_eq!(to_env_name("CLI_", "verbose"), "CLI_VERBOSE");
        assert_eq!(to_env_name("CLI_", "logLevel"), "CLI_LOG_LEVEL");
        assert_eq!(to_env_name("WF_", "maxRetries"), "WF_MAX_RETRIES");
    }

    #[test]
    fn test_env_mapping_builder() {
        let mapping = EnvMappingBuilder::new()
            .string("name", "APP_NAME", Some("default".to_string()))
            .int("count", "APP_COUNT", Some(10))
            .boolean("debug", "APP_DEBUG", Some(false))
            .build();

        assert_eq!(mapping.len(), 3);
        assert!(mapping.contains_key("name"));
        assert!(mapping.contains_key("count"));
        assert!(mapping.contains_key("debug"));
    }

    #[test]
    fn test_expand_env_vars_with_default_and_literal() {
        std::env::set_var("WF_TEST_EXPAND_SET", "secret");
        std::env::remove_var("WF_TEST_EXPAND_MISSING");
        assert_eq!(expand_env_vars("pw=${WF_TEST_EXPAND_SET}"), "pw=secret");
        assert_eq!(
            expand_env_vars("pw=${WF_TEST_EXPAND_MISSING:postgres}"),
            "pw=postgres"
        );
        assert_eq!(
            expand_env_vars("pw=${WF_TEST_EXPAND_MISSING}"),
            "pw=${WF_TEST_EXPAND_MISSING}"
        );
        assert_eq!(expand_env_vars("expr=${input.a}"), "expr=${input.a}");
        std::env::remove_var("WF_TEST_EXPAND_SET");
    }

    #[test]
    fn test_unresolved_env_vars_query() {
        std::env::remove_var("WF_TEST_UNRESOLVED_MISSING");
        assert_eq!(
            find_unresolved_env_vars("pw=${WF_TEST_UNRESOLVED_MISSING}"),
            vec!["WF_TEST_UNRESOLVED_MISSING".to_string()]
        );
        assert!(find_unresolved_env_vars("pw=${WF_TEST_UNRESOLVED_MISSING:fallback}").is_empty());
        assert!(list_env_placeholders("expr=${input.a}").is_empty());
        assert_eq!(
            list_env_placeholders("a=${WF_TEST_UNRESOLVED_MISSING}"),
            vec!["WF_TEST_UNRESOLVED_MISSING".to_string()]
        );
    }
}
