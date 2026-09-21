use serde::{Deserialize, Serialize};

use crate::agent_dbg::{analyze_agent_trace, AgentAnalysis};
use crate::assert::{run_assertions, AssertOutcome};
use crate::model::Trace;
use crate::replay::{replay_trace, ReplayOutcome};
use crate::report::{unify, UnifiedReport};

/// Outcome of the unified check: replay plus every dimension section plus
/// assertions plus agent policy, with one exit code for gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckOutcome {
    pub replay: ReplayOutcome,
    pub report: UnifiedReport,
    pub assertions: AssertOutcome,
    pub agent: AgentAnalysis,
    pub exit_code: i32,
}

pub fn run_check(trace: &Trace, agent_override: Option<&str>) -> CheckOutcome {
    let replay = replay_trace(trace);
    let report = unify(vec![
        crate::loops::analyze(trace),
        crate::merges::analyze(trace),
        crate::interruptions::analyze(trace),
        crate::checkpoints::analyze(trace),
        crate::interactions::analyze(trace),
        crate::subexec::analyze(trace),
        crate::cost::analyze(trace),
    ]);
    let assertions = run_assertions(trace);
    let agent = analyze_agent_trace(trace, agent_override);
    let mut exit_code = 0;
    if assertions.failed() || report.failed() {
        exit_code = 1;
    }
    if agent.known_template && !agent.violations.is_empty() {
        exit_code = 1;
    }
    CheckOutcome {
        replay,
        report,
        assertions,
        agent,
        exit_code,
    }
}

pub fn render_check_text(trace: &Trace, outcome: &CheckOutcome, no_color: bool) -> String {
    let mut buf = crate::format::format_text(trace, &outcome.replay, no_color);
    buf.push_str(&format!(
        "sections={} errors={} warnings={} assertions={}/{}\n",
        outcome.report.sections.len(),
        outcome.report.error_findings,
        outcome.report.warning_findings,
        outcome.assertions.failed,
        outcome.assertions.passed + outcome.assertions.failed,
    ));
    for section in &outcome.report.sections {
        if section.findings.is_empty() && section.counts.is_empty() {
            continue;
        }
        buf.push_str(&format!("== {} ==\n", section.name));
        let mut counts: Vec<(&String, &u64)> = section.counts.iter().collect();
        counts.sort();
        for (key, value) in counts {
            buf.push_str(&format!("  {key}={value}\n"));
        }
        for finding in &section.findings {
            let level = match finding.level {
                crate::report::FindingLevel::Info => "info",
                crate::report::FindingLevel::Warning => "WARN",
                crate::report::FindingLevel::Error => "ERROR",
            };
            buf.push_str(&format!(
                "  [{level}] {}{}\n",
                if finding.path.is_empty() {
                    String::new()
                } else {
                    format!("step-{}: ", finding.path)
                },
                finding.message,
            ));
            if finding.expected.is_some() || finding.actual.is_some() {
                buf.push_str(&format!(
                    "    expected={:?} actual={:?}\n",
                    finding.expected, finding.actual,
                ));
            }
        }
    }
    if !outcome.assertions.results.is_empty() {
        buf.push_str("== assertions ==\n");
        for result in &outcome.assertions.results {
            let status = if result.pass { "PASS" } else { "FAIL" };
            buf.push_str(&format!("  {status} {}\n", result.name));
            if !result.pass {
                buf.push_str(&format!(
                    "    expected={:?} actual={:?} {}\n",
                    result.expected, result.actual, result.message,
                ));
            }
        }
    }
    if outcome.agent.known_template && !outcome.agent.violations.is_empty() {
        buf.push_str(&format!(
            "== agent {} violations={} ==\n",
            outcome.agent.template_id,
            outcome.agent.violations.len(),
        ));
        for violation in &outcome.agent.violations {
            buf.push_str(&format!(
                "  [step-{}] {} {}: {}\n",
                violation.path, violation.tool, violation.kind, violation.detail,
            ));
        }
    }
    buf
}

pub fn render_check_json(trace: &Trace, outcome: &CheckOutcome) -> String {
    let payload = serde_json::json!({
        "schema": trace.schema,
        "kind": trace.kind,
        "summary": outcome.replay.summary,
        "steps": outcome.replay.steps,
        "report": outcome.report,
        "assertions": trace.assertions,
        "assertion_results": outcome.assertions.results,
        "assertions_passed": outcome.assertions.passed,
        "assertions_failed": outcome.assertions.failed,
        "agent": outcome.agent,
        "exit_code": outcome.exit_code,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| String::from("{}"))
}

/// Render the replay command: read-only display, always zero exit.
pub fn render_replay(trace: &Trace, json: bool, no_color: bool) -> (String, i32) {
    let replay = replay_trace(trace);
    if json {
        let assertions = run_assertions(trace);
        (crate::format::format_json(trace, &replay, &assertions), 0)
    } else {
        (crate::format::format_text(trace, &replay, no_color), 0)
    }
}

/// Render the unified check command.
pub fn render_check(
    trace: &Trace,
    agent_override: Option<&str>,
    json: bool,
    no_color: bool,
) -> (String, i32) {
    let outcome = run_check(trace, agent_override);
    let code = outcome.exit_code;
    if json {
        (render_check_json(trace, &outcome), code)
    } else {
        (render_check_text(trace, &outcome, no_color), code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn empty_trace() -> Trace {
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![],
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    #[test]
    fn empty_trace_check_is_clean() {
        let trace = empty_trace();
        let outcome = run_check(&trace, None);
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.report.sections.len(), 7);
    }

    #[test]
    fn check_json_parses_and_carries_report() {
        let trace = empty_trace();
        let outcome = run_check(&trace, None);
        let payload: serde_json::Value =
            serde_json::from_str(&render_check_json(&trace, &outcome)).expect("json renders");
        assert!(payload.get("report").is_some());
        assert!(payload.get("assertion_results").is_some());
    }
}
