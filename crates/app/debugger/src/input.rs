use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::trace::{Trace, TRACE_SCHEMA_V1};
use crate::views::{RouteDecisionPoint, TriggerTemplateView};

/// Single place for trace loading and schema validation. The schema check
/// reports a clear error; traces are never migrated.
pub fn load_trace(path: &Path) -> Result<Trace> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read trace {}", path.display()))?;
    parse_trace(&text)
}

pub fn parse_trace(text: &str) -> Result<Trace> {
    let trace: Trace = serde_json::from_str(text).context("parse trace JSON")?;
    if trace.schema != TRACE_SCHEMA_V1 {
        bail!(
            "unsupported trace schema '{}', expected '{}'",
            trace.schema,
            TRACE_SCHEMA_V1
        );
    }
    Ok(trace)
}

pub fn load_variables(path: Option<&Path>) -> Result<HashMap<String, serde_json::Value>> {
    let Some(path) = path else {
        return Ok(HashMap::new());
    };
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read variables {}", path.display()))?;
    serde_json::from_str(&text).context("parse variables JSON")
}

pub fn load_decision(path: &Path) -> Result<RouteDecisionPoint> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read decision {}", path.display()))?;
    serde_json::from_str(&text).context("parse decision JSON")
}

pub fn load_trigger_templates(path: Option<&Path>) -> Result<Vec<TriggerTemplateView>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read trigger templates {}", path.display()))?;
    serde_json::from_str(&text).context("parse trigger templates JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_schema() {
        let err = parse_trace(r#"{"schema":"other/v9","kind":"workflow","steps":[]}"#)
            .expect_err("unknown schema is rejected");
        assert!(err.to_string().contains("unsupported trace schema"));
    }

    #[test]
    fn accepts_minimal_v1_trace() {
        let trace = parse_trace(r#"{"kind":"agent","steps":[]}"#).expect("v1 parses");
        assert_eq!(trace.schema, TRACE_SCHEMA_V1);
    }
}
