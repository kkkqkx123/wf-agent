use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Severity of a single analyzer conclusion. Only `Error` findings fail the
/// unified check; warnings highlight risks worth inspecting.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub level: FindingLevel,
    /// Walk path of the step the finding belongs to, empty for trace-level.
    #[serde(default)]
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
}

/// One analyzer's contribution to the unified report: its own counters plus
/// the conclusions a human or gate should read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SectionReport {
    pub name: String,
    #[serde(default)]
    pub counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

impl SectionReport {
    pub fn named(name: &str) -> Self {
        SectionReport {
            name: name.to_string(),
            counts: BTreeMap::new(),
            findings: Vec::new(),
        }
    }

    pub fn count(&mut self, key: &str, delta: u64) {
        *self.counts.entry(key.to_string()).or_insert(0) += delta;
    }

    pub fn finding(
        &mut self,
        level: FindingLevel,
        path: &str,
        message: String,
        expected: Option<serde_json::Value>,
        actual: Option<serde_json::Value>,
    ) {
        self.findings.push(Finding {
            level,
            path: path.to_string(),
            message,
            expected,
            actual,
        });
    }

    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.level == FindingLevel::Error)
            .count()
    }
}

/// Merged output of every analyzer. Text and JSON rendering consume only
/// this struct so both shapes stay in sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UnifiedReport {
    pub sections: Vec<SectionReport>,
    /// Cross-dimension coverage counters (rounds, branches, templates...).
    #[serde(default)]
    pub coverage: BTreeMap<String, u64>,
    pub error_findings: usize,
    pub warning_findings: usize,
}

impl UnifiedReport {
    pub fn failed(&self) -> bool {
        self.error_findings > 0
    }
}

pub fn unify(sections: Vec<SectionReport>) -> UnifiedReport {
    let mut coverage = BTreeMap::new();
    let mut errors = 0;
    let mut warnings = 0;
    for section in &sections {
        for (key, value) in &section.counts {
            *coverage
                .entry(format!("{}.{}", section.name, key))
                .or_insert(0) += value;
        }
        for finding in &section.findings {
            match finding.level {
                FindingLevel::Error => errors += 1,
                FindingLevel::Warning => warnings += 1,
                FindingLevel::Info => {}
            }
        }
    }
    UnifiedReport {
        sections,
        coverage,
        error_findings: errors,
        warning_findings: warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unify_aggregates_counts_and_severities() {
        let mut section = SectionReport::named("loops");
        section.count("rounds", 2);
        section.finding(
            FindingLevel::Error,
            "0",
            "round failed".to_string(),
            None,
            None,
        );
        let report = unify(vec![section]);
        assert_eq!(report.coverage.get("loops.rounds"), Some(&2));
        assert!(report.failed());
        assert_eq!(report.warning_findings, 0);
    }
}
