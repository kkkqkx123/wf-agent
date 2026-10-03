use std::collections::{BTreeMap, BTreeSet};

use crate::model::report::{FindingLevel, SectionReport};
use crate::model::trace::Trace;
use crate::model::traverse::walk;
use crate::model::views_cost::{CompressionPhase, CompressionView};

fn event_key(target: &str, version: u64) -> String {
    format!("{target}#{version}")
}

/// Record the run identity of one terminal event: a second distinct run id
/// for the same key means a redundant summary run (late duplicate after an
/// eviction or a cross-version claim overlap) whose write-back the version
/// anchor discards. Stays silent when either side predates the run key.
fn note_terminal_run(
    report: &mut SectionReport,
    terminal_runs: &mut BTreeMap<String, BTreeSet<String>>,
    key: &str,
    view: &CompressionView,
    path: &str,
) {
    let Some(run_id) = view.run_id.clone() else {
        return;
    };
    let seen = terminal_runs.entry(key.to_string()).or_default();
    if !seen.is_empty() && !seen.contains(&run_id) {
        report.count("redundant_summary_run", 1);
        report.finding(
            FindingLevel::Info,
            path,
            format!(
                "redundant summary run for '{}' at version {} (run '{run_id}' overlaps an earlier run; the version anchor discards the loser)",
                view.target_context_id, view.array_version,
            ),
            None,
            Some(serde_json::Value::String(run_id.clone())),
        );
    }
    seen.insert(run_id);
}

/// Context-compression lifecycle analysis.
///
/// Groups `requested` events with their exactly-one terminal event
/// (`completed`, `failed` or `discarded`) per `(target_context_id,
/// array_version)` and surfaces the failure modes the engine itself
/// distinguishes: terminal failures (emitter parks for external handling),
/// degraded partial windows (oldest messages dropped without a summary),
/// still-over-budget results (backed off for one version; consecutive loops
/// escalate) and requests without a terminal event (lost completion blocks
/// the emitter until its settle timeout). In routed mode the adapter
/// publishes a `routed` handoff between request and terminal: duplicate
/// handoffs lose the pipeline claim race (info), a handoff with no emitter
/// request is out-of-band (warning), and a terminal pairs against either.
/// Terminal events carrying distinct run ids for the same key mark redundant
/// summary runs whose write-back the version anchor discards.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("compression");
    let mut requested: BTreeSet<String> = BTreeSet::new();
    let mut handoff: BTreeSet<String> = BTreeSet::new();
    let mut handoff_paths: BTreeMap<String, String> = BTreeMap::new();
    let mut terminal: BTreeMap<String, CompressionPhase> = BTreeMap::new();
    let mut terminal_runs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut completed_without_request: Vec<(String, CompressionView)> = Vec::new();
    let mut no_taker_per_target: BTreeMap<String, usize> = BTreeMap::new();
    let mut still_over_per_target: BTreeMap<String, usize> = BTreeMap::new();

    for visit in walk(trace) {
        for view in &visit.step.compressions {
            let key = event_key(&view.target_context_id, view.array_version);
            match view.phase {
                CompressionPhase::Requested => {
                    report.count("requested", 1);
                    report.count(&format!("target:{}:requested", view.target_context_id), 1);
                    if view.forced {
                        report.count("forced", 1);
                    }
                    if view.budget_unknown {
                        report.count("budget_unknown", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "forced compression for '{}' at version {} has no known context budget",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    }
                    if view.no_taker {
                        report.count("no_taker", 1);
                        let streak = no_taker_per_target
                            .get(&view.target_context_id)
                            .copied()
                            .unwrap_or(0)
                            + 1;
                        no_taker_per_target.insert(view.target_context_id.clone(), streak);
                        if streak >= 3 {
                            report.finding(
                                FindingLevel::Error,
                                &visit.path,
                                format!(
                                    "compression signal for '{}' at version {} repeatedly has no taker ({} consecutive; compression service missing)",
                                    view.target_context_id, view.array_version, streak,
                                ),
                                None,
                                None,
                            );
                        } else {
                            report.finding(
                                FindingLevel::Warning,
                                &visit.path,
                                format!(
                                    "compression signal for '{}' at version {} had no taker (audit event kept, flight not anchored; the emitter proceeds without compression)",
                                    view.target_context_id, view.array_version,
                                ),
                                None,
                                None,
                            );
                        }
                    } else {
                        // A dispatched signal resets the consecutive streak,
                        // mirroring the runtime tracker.
                        no_taker_per_target.remove(&view.target_context_id);
                    }
                    if requested.contains(&key) {
                        report.count("duplicate_requested", 1);
                        report.finding(
                            FindingLevel::Info,
                            &visit.path,
                            format!(
                                "duplicate compression request for '{}' at version {} (dedup skips the rerun)",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    } else {
                        requested.insert(key.clone());
                    }
                    if terminal.contains_key(&key) {
                        report.count("request_after_terminal", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "compression requested for '{}' at version {} after its terminal event already landed",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    }
                }
                CompressionPhase::Routed => {
                    report.count("routed", 1);
                    report.count(&format!("target:{}:routed", view.target_context_id), 1);
                    if terminal.contains_key(&key) {
                        report.count("handoff_after_terminal", 1);
                        report.finding(
                            FindingLevel::Info,
                            &visit.path,
                            format!(
                                "compression handoff for '{}' at version {} arrived after its terminal event already landed (late duplicate; the claim race drops it)",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    } else if handoff.contains(&key) {
                        report.count("duplicate_routed", 1);
                        report.finding(
                            FindingLevel::Info,
                            &visit.path,
                            format!(
                                "duplicate compression handoff for '{}' at version {} (the pipeline claim race drops the rerun)",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    } else {
                        handoff.insert(key.clone());
                        handoff_paths.insert(key.clone(), visit.path.clone());
                    }
                }
                CompressionPhase::Completed => {
                    report.count("completed", 1);
                    report.count(&format!("target:{}:completed", view.target_context_id), 1);
                    if terminal
                        .insert(key.clone(), CompressionPhase::Completed)
                        .is_some()
                    {
                        report.count("duplicate_terminal", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "duplicate terminal compression event for '{}' at version {}",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    }
                    note_terminal_run(&mut report, &mut terminal_runs, &key, view, &visit.path);
                    if view.degraded {
                        report.count("degraded", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "compression for '{}' at version {} degraded to a partial window without a summary ({} dropped){}",
                                view.target_context_id,
                                view.array_version,
                                view.degraded_dropped,
                                view.error
                                    .as_deref()
                                    .map(|error| format!(": {error}"))
                                    .unwrap_or_default(),
                            ),
                            None,
                            None,
                        );
                    }
                    if view.still_over_budget {
                        report.count("still_over_budget", 1);
                        let streak = still_over_per_target
                            .get(&view.target_context_id)
                            .copied()
                            .unwrap_or(0)
                            + 1;
                        still_over_per_target.insert(view.target_context_id.clone(), streak);
                        if streak >= 3 {
                            report.finding(
                                FindingLevel::Error,
                                &visit.path,
                                format!(
                                    "compressed results for '{}' still exceed budget {} times consecutively (latest v{} tokens_after={}); check budget or content compressibility",
                                    view.target_context_id,
                                    streak,
                                    view.array_version,
                                    view.tokens_after
                                        .map(|tokens| tokens.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                ),
                                view.token_limit.map(serde_json::Value::from),
                                view.tokens_after.map(serde_json::Value::from),
                            );
                        } else {
                            report.finding(
                                FindingLevel::Warning,
                                &visit.path,
                                format!(
                                    "compressed result for '{}' at version {} still exceeds its budget (tokens_after={}); backoff suppresses an immediate retrigger for the landed version",
                                    view.target_context_id,
                                    view.array_version,
                                    view.tokens_after
                                        .map(|tokens| tokens.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                ),
                                view.token_limit.map(serde_json::Value::from),
                                view.tokens_after.map(serde_json::Value::from),
                            );
                        }
                    } else {
                        // A fitting result resets the consecutive streak,
                        // mirroring the runtime tracker.
                        still_over_per_target.remove(&view.target_context_id);
                    }
                    let summary_empty = view
                        .summary
                        .as_deref()
                        .is_none_or(|summary| summary.trim().is_empty());
                    if summary_empty && !view.degraded {
                        report.count("missing_summary", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "compression completed for '{}' at version {} carries no summary text",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    }
                    if !requested.contains(&key) && !handoff.contains(&key) {
                        completed_without_request.push((visit.path.clone(), view.clone()));
                    }
                }
                CompressionPhase::Failed => {
                    report.count("failed", 1);
                    report.count(&format!("target:{}:failed", view.target_context_id), 1);
                    if terminal
                        .insert(key.clone(), CompressionPhase::Failed)
                        .is_some()
                    {
                        report.count("duplicate_terminal", 1);
                    }
                    note_terminal_run(&mut report, &mut terminal_runs, &key, view, &visit.path);
                    report.finding(
                        FindingLevel::Error,
                        &visit.path,
                        format!(
                            "compression failed for '{}' at version {}: {} (attempts {})",
                            view.target_context_id,
                            view.array_version,
                            view.error.clone().unwrap_or_else(|| "unknown".to_string()),
                            view.attempts.unwrap_or(0),
                        ),
                        None,
                        view.error.clone().map(serde_json::Value::String),
                    );
                    if !requested.contains(&key) && !handoff.contains(&key) {
                        completed_without_request.push((visit.path.clone(), view.clone()));
                    }
                }
                CompressionPhase::Discarded => {
                    report.count("discarded", 1);
                    report.count(&format!("target:{}:discarded", view.target_context_id), 1);
                    if terminal
                        .insert(key.clone(), CompressionPhase::Discarded)
                        .is_some()
                    {
                        report.count("duplicate_terminal", 1);
                        report.finding(
                            FindingLevel::Warning,
                            &visit.path,
                            format!(
                                "duplicate terminal compression event for '{}' at version {}",
                                view.target_context_id, view.array_version,
                            ),
                            None,
                            None,
                        );
                    }
                    note_terminal_run(&mut report, &mut terminal_runs, &key, view, &visit.path);
                    report.finding(
                        FindingLevel::Info,
                        &visit.path,
                        format!(
                            "compression result for '{}' at version {} discarded ({})",
                            view.target_context_id,
                            view.array_version,
                            view.discard_reason
                                .clone()
                                .unwrap_or_else(|| "stale version".to_string()),
                        ),
                        view.current_version.map(serde_json::Value::from),
                        None,
                    );
                    if !requested.contains(&key) && !handoff.contains(&key) {
                        completed_without_request.push((visit.path.clone(), view.clone()));
                    }
                }
            }
        }
        // Hook-payload backfill at step level: a compression signal encoded
        // only as a hook fire without an explicit compression record. This
        // must run even when the step carries no compression views, so it
        // lives outside the view loop above.
        if visit.step.compressions.is_empty() {
            for hook in &visit.step.hooks_fired {
                if hook.hook_type == "CONTEXT_COMPRESSION_REQUESTED" {
                    report.count("hook_signal_without_record", 1);
                    report.finding(
                        FindingLevel::Info,
                        &visit.path,
                        format!(
                            "compression signal present only as hook payload '{}' with no explicit compression record",
                            hook.hook_id,
                        ),
                        None,
                        None,
                    );
                    break;
                }
            }
        }
    }

    for (path, view) in completed_without_request {
        report.count("terminal_without_request", 1);
        report.finding(
            FindingLevel::Warning,
            &path,
            format!(
                "compression {} for '{}' at version {} has no matching request (stale or out-of-band)",
                view.phase.label(),
                view.target_context_id,
                view.array_version,
            ),
            None,
            None,
        );
    }

    // Handoffs with no emitter request are out-of-band: the adapter only
    // publishes after an emission, so the audit copy was lost. Deferred past
    // the walk so trace order never false-positives.
    let mut orphaned: Vec<(&String, &String)> = handoff_paths
        .iter()
        .filter(|(key, _)| !requested.contains(*key))
        .collect();
    orphaned.sort();
    for (key, path) in orphaned {
        report.count("routed_without_request", 1);
        let (target, version) = key
            .rsplit_once('#')
            .map(|(target, version)| (target, version.parse::<u64>().unwrap_or(0)))
            .unwrap_or((key.as_str(), 0));
        report.finding(
            FindingLevel::Warning,
            path,
            format!(
                "compression handoff for '{target}' at version {version} has no matching emitter request (out-of-band handoff)"
            ),
            None,
            None,
        );
    }

    let mut missing: Vec<String> = requested
        .iter()
        .filter(|key| !terminal.contains_key(*key))
        .cloned()
        .collect();
    missing.sort();
    for key in missing {
        report.count("missing_terminal", 1);
        let (target, version) = key
            .rsplit_once('#')
            .map(|(target, version)| (target, version.parse::<u64>().unwrap_or(0)))
            .unwrap_or((key.as_str(), 0));
        report.finding(
            FindingLevel::Warning,
            "",
            format!(
                "compression requested for '{target}' at version {version} has no terminal event (the emitter waits until its settle timeout)"
            ),
            None,
            None,
        );
    }

    if !report.counts.contains_key("requested") {
        report.count("requested", 0);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn step_with(index: usize, compressions: Vec<CompressionView>) -> StepRecord {
        StepRecord {
            index,
            node_id: "llm-1".to_string(),
            node_name: String::new(),
            node_type: "LLM".to_string(),
            input: serde_json::Value::Null,
            result: serde_json::Value::Null,
            success: true,
            error: None,
            error_kind: None,
            retryable: None,
            recovery_hint: None,
            branch_id: None,
            route_target: None,
            start_time: None,
            end_time: None,
            variable_before: HashMap::new(),
            variable_after: HashMap::new(),
            messages_before: HashMap::new(),
            messages_after: HashMap::new(),
            tool_calls: vec![],
            llm_calls: vec![],
            approval: None,
            visibility: None,
            loop_round: None,
            merge: None,
            interruption: None,
            checkpoint: None,
            interaction: None,
            hooks_fired: vec![],
            triggers_seen: vec![],
            compressions,
            exec_id: None,
            parent_exec_id: None,
            root_exec_id: None,
            depth: None,
            result_var: None,
            wait_for_child: None,
            child_timeout_ms: None,
            dialog_anchor: None,
            writeback: None,
            children: vec![],
        }
    }

    fn requested(target: &str, version: u64) -> CompressionView {
        CompressionView {
            target_context_id: target.to_string(),
            phase: CompressionPhase::Requested,
            array_version: version,
            tokens_used: Some(1200),
            token_limit: Some(1000),
            message_count: Some(40),
            tokens_after: None,
            tail_keep: None,
            forced: false,
            budget_unknown: false,
            no_taker: false,
            degraded: false,
            degraded_dropped: 0,
            still_over_budget: false,
            summary: None,
            error: None,
            attempts: None,
            discard_reason: None,
            current_version: None,
            run_id: None,
        }
    }

    fn completed(target: &str, version: u64) -> CompressionView {
        CompressionView {
            target_context_id: target.to_string(),
            phase: CompressionPhase::Completed,
            array_version: version,
            tokens_used: None,
            token_limit: Some(1000),
            message_count: None,
            tokens_after: Some(300),
            tail_keep: Some(2),
            forced: false,
            budget_unknown: false,
            no_taker: false,
            degraded: false,
            degraded_dropped: 0,
            still_over_budget: false,
            summary: Some("summary".to_string()),
            error: None,
            attempts: None,
            discard_reason: None,
            current_version: None,
            run_id: None,
        }
    }

    fn trace_with(steps: Vec<StepRecord>) -> Trace {
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Workflow,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps,
            assertions: vec![],
            trigger_templates: vec![],
            budget: None,
        }
    }

    #[test]
    fn matched_request_and_completion_is_clean() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(1, vec![completed("chat", 7)]),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("requested"), Some(&1));
        assert_eq!(report.counts.get("completed"), Some(&1));
        assert_eq!(report.errors(), 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn failed_completion_is_an_error() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(
                1,
                vec![CompressionView {
                    phase: CompressionPhase::Failed,
                    attempts: Some(2),
                    error: Some("timed out".to_string()),
                    ..requested("chat", 7)
                }],
            ),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("failed"), Some(&1));
        assert_eq!(report.errors(), 1);
    }

    #[test]
    fn degraded_and_still_over_budget_warn() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 3)]),
            step_with(
                1,
                vec![CompressionView {
                    phase: CompressionPhase::Completed,
                    degraded: true,
                    still_over_budget: true,
                    tokens_after: Some(1500),
                    token_limit: Some(1000),
                    summary: None,
                    ..completed("chat", 3)
                }],
            ),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("degraded"), Some(&1));
        assert_eq!(report.counts.get("still_over_budget"), Some(&1));
        assert_eq!(report.errors(), 0);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn missing_terminal_warns() {
        let trace = trace_with(vec![step_with(0, vec![requested("chat", 9)])]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("missing_terminal"), Some(&1));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn terminal_without_request_warns() {
        let trace = trace_with(vec![step_with(0, vec![completed("chat", 4)])]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("terminal_without_request"), Some(&1));
    }

    #[test]
    fn forced_without_budget_warns() {
        let trace = trace_with(vec![step_with(
            0,
            vec![CompressionView {
                forced: true,
                budget_unknown: true,
                token_limit: Some(0),
                ..requested("chat", 1)
            }],
        )]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("forced"), Some(&1));
        assert_eq!(report.counts.get("budget_unknown"), Some(&1));
    }

    #[test]
    fn signal_without_taker_warns() {
        let trace = trace_with(vec![step_with(
            0,
            vec![CompressionView {
                no_taker: true,
                ..requested("chat", 2)
            }],
        )]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("no_taker"), Some(&1));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn distinct_run_ids_for_same_version_flag_redundant_run() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(
                1,
                vec![CompressionView {
                    run_id: Some("run-1".to_string()),
                    ..completed("chat", 7)
                }],
            ),
            step_with(
                2,
                vec![CompressionView {
                    run_id: Some("run-2".to_string()),
                    ..completed("chat", 7)
                }],
            ),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("duplicate_terminal"), Some(&1));
        assert_eq!(report.counts.get("redundant_summary_run"), Some(&1));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn repeated_run_id_stays_clean() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(
                1,
                vec![CompressionView {
                    run_id: Some("run-1".to_string()),
                    ..completed("chat", 7)
                }],
            ),
        ]);
        let report = analyze(&trace);
        assert!(!report.counts.contains_key("redundant_summary_run"));
        assert!(report.findings.is_empty());
    }

    fn routed(target: &str, version: u64) -> CompressionView {
        CompressionView {
            phase: CompressionPhase::Routed,
            ..requested(target, version)
        }
    }

    #[test]
    fn routed_handoff_pairs_request_with_terminal_cleanly() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(1, vec![routed("chat", 7)]),
            step_with(
                2,
                vec![CompressionView {
                    run_id: Some("run-1".to_string()),
                    ..completed("chat", 7)
                }],
            ),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("requested"), Some(&1));
        assert_eq!(report.counts.get("routed"), Some(&1));
        assert_eq!(report.counts.get("completed"), Some(&1));
        assert_eq!(report.errors(), 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn duplicate_routed_handoff_is_info() {
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 7)]),
            step_with(1, vec![routed("chat", 7)]),
            step_with(2, vec![routed("chat", 7)]),
            step_with(3, vec![completed("chat", 7)]),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("duplicate_routed"), Some(&1));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn routed_without_request_warns_but_pairs_terminal() {
        let trace = trace_with(vec![
            step_with(0, vec![routed("chat", 4)]),
            step_with(1, vec![completed("chat", 4)]),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("routed_without_request"), Some(&1));
        // The handoff satisfies terminal pairing: no terminal_without_request.
        assert!(!report.counts.contains_key("terminal_without_request"));
    }

    #[test]
    fn routed_without_terminal_still_warns_missing_terminal() {
        // Requested but the handoff never landed a terminal: the emitter
        // still waits, so the missing-terminal warning stays.
        let trace = trace_with(vec![
            step_with(0, vec![requested("chat", 9)]),
            step_with(1, vec![routed("chat", 9)]),
        ]);
        let report = analyze(&trace);
        assert_eq!(report.counts.get("missing_terminal"), Some(&1));
    }
}
