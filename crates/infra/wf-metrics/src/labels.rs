use std::collections::{HashMap, HashSet};

use wf_types::config::metrics::MetricCollectorConfig;

/// Label governance for a collector.
///
/// Absent allowlist means every key is accepted. Violations warn by default
/// and drop only in strict mode, so existing collectors keep working until
/// an explicit allowlist is configured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelConfig {
    pub allowed_keys: Option<HashSet<String>>,
    pub strict: bool,
}

impl From<&MetricCollectorConfig> for LabelConfig {
    fn from(cfg: &MetricCollectorConfig) -> Self {
        Self {
            allowed_keys: cfg
                .allowed_label_keys
                .as_ref()
                .map(|keys| keys.iter().cloned().collect::<HashSet<String>>()),
            strict: cfg.strict_labels.unwrap_or(false),
        }
    }
}

impl LabelConfig {
    /// Check a label set against the allowlist. Invalid keys are logged;
    /// the return value tells the caller whether the series may be recorded.
    pub fn validate(&self, metric_name: &str, labels: &HashMap<String, String>) -> bool {
        let Some(allowed) = self.allowed_keys.as_ref() else {
            return true;
        };
        let mut valid = true;
        for key in labels.keys() {
            if !allowed.contains(key) {
                valid = false;
                tracing::warn!(
                    target: "wf_metrics",
                    metric = metric_name,
                    label = key.as_str(),
                    "label key outside allowlist"
                );
            }
        }
        if valid {
            return true;
        }
        if self.strict {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::labels as make_labels;

    fn config(strict: bool, keys: &[&str]) -> LabelConfig {
        LabelConfig {
            allowed_keys: Some(keys.iter().map(|k| k.to_string()).collect()),
            strict,
        }
    }

    #[test]
    fn permissive_by_default() {
        let cfg = LabelConfig::default();
        assert!(cfg.validate("m", &make_labels(&[("anything", "v")])));
    }

    #[test]
    fn warn_mode_keeps_series() {
        let cfg = config(false, &["env"]);
        assert!(cfg.validate("m", &make_labels(&[("other", "v")])));
    }

    #[test]
    fn strict_mode_drops_series() {
        let cfg = config(true, &["env"]);
        assert!(!cfg.validate("m", &make_labels(&[("other", "v")])));
        assert!(cfg.validate("m", &make_labels(&[("env", "prod")])));
    }

    #[test]
    fn maps_from_collector_config() {
        let cfg: LabelConfig = (&MetricCollectorConfig {
            strict_labels: Some(true),
            allowed_label_keys: Some(vec!["env".to_string()]),
            ..Default::default()
        })
            .into();
        assert!(cfg.strict);
        assert!(cfg.allowed_keys.as_ref().is_some_and(|k| k.contains("env")));
    }
}
