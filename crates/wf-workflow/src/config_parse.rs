//! Explicit node-config parsing helpers (Phase 3 / B11).
//!
//! Replaces the scattered `serde_json::from_value(...).ok()` /
//! `unwrap_or_default()` sites that silently degraded invalid user config:
//! - `parse_node_config` fails with a structured `ConfigError` carrying the
//!   node id and the field path (semantic config must not silently fall back).
//! - `parse_node_config_or_warn` degrades to a default but logs a warning so
//!   the invalidity stays observable (optional enhancement config).

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{WorkflowError, WorkflowResult};

/// Strictly parse `value` as `T`. On failure, returns a `ConfigError`
/// carrying the node id, the field path and the serde detail.
pub fn parse_node_config<T: DeserializeOwned>(
    node_id: &str,
    field: &str,
    value: &Value,
) -> WorkflowResult<T> {
    serde_json::from_value(value.clone()).map_err(|e| WorkflowError::ConfigError {
        node_id: node_id.to_string(),
        field: field.to_string(),
        detail: e.to_string(),
    })
}

/// Parse `value` as `T`, degrading to `default` on failure with an explicit
/// warning (node id + field path in the log line).
pub fn parse_node_config_or_warn<T: DeserializeOwned>(
    node_id: &str,
    field: &str,
    value: &Value,
    default: T,
) -> T {
    match serde_json::from_value(value.clone()) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!(
                node_id,
                field,
                error = %e,
                "invalid node config, falling back to default"
            );
            default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct SampleConfig {
        enabled: bool,
        count: u32,
    }

    #[test]
    fn strict_parse_fails_with_node_id_and_field() {
        let err = parse_node_config::<SampleConfig>("n7", "inner.retry_policy", &Value::Null)
            .expect_err("null must not parse");
        match err {
            crate::error::WorkflowError::ConfigError {
                node_id,
                field,
                detail,
            } => {
                assert_eq!(node_id, "n7");
                assert_eq!(field, "inner.retry_policy");
                assert!(!detail.is_empty());
            }
            other => panic!("expected ConfigError, got {other:?}"),
        }
    }

    #[test]
    fn strict_parse_succeeds_on_valid_value() {
        let parsed = parse_node_config::<SampleConfig>(
            "n7",
            "inner.x",
            &serde_json::json!({"enabled": true, "count": 3}),
        )
        .expect("valid config parses");
        assert_eq!(
            parsed,
            SampleConfig {
                enabled: true,
                count: 3
            }
        );
    }

    #[test]
    fn warn_parse_degrades_with_default() {
        let degraded = parse_node_config_or_warn::<SampleConfig>(
            "n7",
            "inner.violation_policy",
            &Value::Null,
            SampleConfig {
                enabled: false,
                count: 0,
            },
        );
        assert_eq!(
            degraded,
            SampleConfig {
                enabled: false,
                count: 0
            }
        );
    }
}
