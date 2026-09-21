use std::collections::BTreeMap;

use crate::report::{FindingLevel, SectionReport};
use crate::trace::Trace;
use crate::traverse::walk;

/// LLM cost analysis: aggregate usage per model and profile, keep estimated
/// calls out of budget decisions, and check trace-level budget limits.
pub fn analyze(trace: &Trace) -> SectionReport {
    let mut report = SectionReport::named("cost");
    let mut reported_tokens: u64 = 0;
    let mut reported_cost: f64 = 0.0;
    let mut per_model: BTreeMap<String, u64> = BTreeMap::new();
    for visit in walk(trace) {
        for call in &visit.step.llm_calls {
            report.count("llm_calls", 1);
            let total = call.effective_total();
            let model = call
                .model
                .clone()
                .unwrap_or_else(|| "unknown-model".to_string());
            *per_model.entry(model).or_insert(0) += total;
            if call.estimated {
                report.count("estimated_calls", 1);
                report.count("estimated_tokens", total);
                continue;
            }
            report.count("reported_calls", 1);
            report.count("prompt_tokens", u64::from(call.prompt_tokens));
            report.count("completion_tokens", u64::from(call.completion_tokens));
            report.count("total_tokens", total);
            report.count(
                "reasoning_tokens",
                u64::from(call.reasoning_tokens.unwrap_or(0)),
            );
            report.count(
                "cache_read_tokens",
                u64::from(call.cache_read_tokens.unwrap_or(0)),
            );
            report.count(
                "cache_write_tokens",
                u64::from(call.cache_write_tokens.unwrap_or(0)),
            );
            reported_tokens += total;
            if let Some(cost) = call.total_cost {
                reported_cost += cost;
            }
            if call.error.is_some() {
                report.count("failed_calls", 1);
                report.finding(
                    FindingLevel::Warning,
                    &visit.path,
                    format!("llm call on profile '{}' failed", call.profile_id),
                    None,
                    call.error.clone().map(serde_json::Value::String),
                );
            }
        }
    }
    for (model, tokens) in &per_model {
        report.count(&format!("model:{model}:tokens"), *tokens);
    }
    if let Some(budget) = trace.budget.as_ref() {
        if let Some(limit) = budget.limit_tokens {
            if reported_tokens > limit {
                report.finding(
                    FindingLevel::Error,
                    "",
                    format!("token budget exceeded: used {reported_tokens} of {limit}"),
                    Some(serde_json::Value::from(limit)),
                    Some(serde_json::Value::from(reported_tokens)),
                );
            } else if let Some(warn_at) = budget.warn_at {
                let ratio = reported_tokens as f64 / limit.max(1) as f64;
                if ratio >= warn_at {
                    report.finding(
                        FindingLevel::Warning,
                        "",
                        format!(
                            "token budget at {:.1}% of limit ({reported_tokens}/{limit})",
                            ratio * 100.0,
                        ),
                        Some(serde_json::Value::from(limit)),
                        Some(serde_json::Value::from(reported_tokens)),
                    );
                }
            }
        }
        if let Some(limit) = budget.limit_cost {
            if reported_cost > limit {
                report.finding(
                    FindingLevel::Error,
                    "",
                    format!("cost budget exceeded: spent {reported_cost:.4} of {limit:.4}"),
                    Some(serde_json::json!(limit)),
                    Some(serde_json::json!(reported_cost)),
                );
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BudgetView, LlmCallView, StepRecord, TraceKind, TRACE_SCHEMA_V1};
    use std::collections::HashMap;

    fn trace_with(calls: Vec<LlmCallView>, budget: Option<BudgetView>) -> Trace {
        let step = StepRecord {
            index: 0,
            node_id: "n".to_string(),
            node_name: String::new(),
            node_type: "AGENT".to_string(),
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
            llm_calls: calls,
            approval: None,
            visibility: None,
            loop_round: None,
            merge: None,
            interruption: None,
            checkpoint: None,
            interaction: None,
            hooks_fired: vec![],
            triggers_seen: vec![],
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
        };
        Trace {
            schema: TRACE_SCHEMA_V1.to_string(),
            kind: TraceKind::Agent,
            graph_ref: String::new(),
            agent_template: String::new(),
            initial_variables: HashMap::new(),
            steps: vec![step],
            assertions: vec![],
            trigger_templates: vec![],
            budget,
        }
    }

    fn call(prompt: u32, completion: u32, estimated: bool) -> LlmCallView {
        LlmCallView {
            profile_id: "default".to_string(),
            model: Some("test-model".to_string()),
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_cost: Some(0.001),
            estimated,
            content_preview: None,
            tool_call_count: 0,
            error: None,
        }
    }

    #[test]
    fn estimates_do_not_feed_budget_totals() {
        let trace = trace_with(
            vec![call(100, 50, false), call(1000, 500, true)],
            Some(BudgetView {
                source: Some("test".to_string()),
                limit_tokens: Some(200),
                limit_cost: None,
                warn_at: None,
            }),
        );
        let report = analyze(&trace);
        assert_eq!(report.counts.get("total_tokens"), Some(&150));
        assert_eq!(report.counts.get("estimated_tokens"), Some(&1500));
        assert_eq!(report.errors(), 0);
    }

    #[test]
    fn budget_overrun_is_an_error() {
        let trace = trace_with(
            vec![call(100, 50, false)],
            Some(BudgetView {
                source: None,
                limit_tokens: Some(100),
                limit_cost: None,
                warn_at: None,
            }),
        );
        let report = analyze(&trace);
        assert_eq!(report.errors(), 1);
    }
}
