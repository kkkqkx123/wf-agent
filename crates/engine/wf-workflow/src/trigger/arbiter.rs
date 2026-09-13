//! Competition-scope arbitration: turn the candidate templates that matched
//! one event into the winners that may be dispatched.
//!
//! Matching (`crate::trigger::matcher`) is per template; several templates can
//! still answer the same event instance, and they compete. This module owns
//! that resolution: per-scope winner selection, the loud rejection of scopes
//! whose declarations should never have passed load-time validation, and the
//! dispatch burst cap that bounds how many winners one event may execute.

use std::collections::HashMap;

use tracing::warn;
use wf_types::events::BaseEvent;
use wf_types::trigger::{self, ResolvedScopeMode, TriggerRuntimeLimits, TriggerTemplate};

/// Resolve candidate templates into winners, one per competition scope.
///
/// A unique scope contributes its single subscriber, a best-win scope
/// contributes its highest explicit priority. Scopes that fail validation
/// (several unique subscribers, missing or duplicate best-win priorities,
/// mixed dispatch modes) contribute no winner and are dropped loudly. At most
/// one scope can match one event instance, so more than one scope winner for
/// the same event means validation was bypassed: the multi-scope policy
/// decides (`DropAll` drops every winner loudly; `KeepFirst` keeps the
/// deterministically-first winner and drops the rest loudly). The dispatch
/// burst cap, when set, applies after deterministic sorting and reports the
/// rest.
pub(crate) fn arbitrate(
    candidates: &[TriggerTemplate],
    event: &BaseEvent,
    limits: &TriggerRuntimeLimits,
) -> Vec<TriggerTemplate> {
    let mut winners: Vec<TriggerTemplate> = Vec::new();
    for group in trigger::scope_groups(candidates) {
        let members: Vec<TriggerTemplate> = group.iter().map(|&i| candidates[i].clone()).collect();
        let refs: Vec<&TriggerTemplate> = members.iter().collect();
        match trigger::resolve_scope_mode(&refs) {
            Err((unique, best_win)) => {
                let unique_names: Vec<&str> = unique.iter().map(|t| t.name.as_str()).collect();
                let best_win_names: Vec<&str> = best_win.iter().map(|t| t.name.as_str()).collect();
                warn!(
                    "Trigger scope for event {} mixes unique declarations [{}] with best-win declarations [{}]; validation was bypassed, dropping the scope",
                    event.r#type.as_str(),
                    unique_names.join(", "),
                    best_win_names.join(", "),
                );
            }
            Ok(ResolvedScopeMode::Unique) => {
                if members.len() > 1 {
                    let names: Vec<&str> = members.iter().map(|t| t.name.as_str()).collect();
                    warn!(
                        "Several trigger templates [{}] matched {} under unique dispatch; validation was bypassed, dropping the scope",
                        names.join(", "),
                        event.r#type.as_str(),
                    );
                } else if let Some(winner) = members.into_iter().next() {
                    winners.push(winner);
                }
            }
            Ok(ResolvedScopeMode::BestWin) => {
                if let Some(winner) = best_win_winner(&members, event) {
                    winners.push(winner);
                }
            }
        }
    }

    if winners.len() <= 1 {
        return winners;
    }
    trigger::sort_winners_deterministically(&mut winners);
    let matched_scopes: Vec<String> = winners
        .iter()
        .map(|t| t.name.as_str().to_string())
        .collect();
    if limits.dispatch_burst_limit.is_some() {
        let dropped = limits.apply_burst_cap(&mut winners);
        let kept: Vec<&str> = winners.iter().map(|t| t.name.as_str()).collect();
        warn!(
            "Several trigger scopes [{}] matched {} for one event; dispatch burst cap dropped {} winner(s), executing [{}]",
            matched_scopes.join(", "),
            event.r#type.as_str(),
            dropped,
            kept.join(", "),
        );
        return winners;
    }
    match limits.multi_scope_policy() {
        trigger::MultiScopePolicy::KeepFirst => {
            let kept = winners[0].name.clone();
            let dropped: Vec<&str> = winners[1..].iter().map(|t| t.name.as_str()).collect();
            warn!(
                "Several trigger scopes [{}] matched {} for one event; multi-scope policy keep_first keeps [{}], dropping [{}]",
                matched_scopes.join(", "),
                event.r#type.as_str(),
                kept,
                dropped.join(", "),
            );
            winners.truncate(1);
            return winners;
        }
        trigger::MultiScopePolicy::DropAll => {}
    }
    warn!(
        "Several trigger scopes [{}] matched {} for one event; multi-scope policy drop_all drops every winner",
        matched_scopes.join(", "),
        event.r#type.as_str(),
    );
    Vec::new()
}

/// Highest explicit priority of a best-win scope. Best-win scopes require an
/// explicit distinct priority per subscriber; anything else means validation
/// was bypassed, so the scope is dropped without falling back to implicit
/// ordering.
fn best_win_winner(members: &[TriggerTemplate], event: &BaseEvent) -> Option<TriggerTemplate> {
    let missing: Vec<&str> = members
        .iter()
        .filter(|t| t.priority.is_none())
        .map(|t| t.name.as_str())
        .collect();
    if !missing.is_empty() {
        warn!(
            "Trigger templates [{}] matched {} under best-win dispatch without explicit priorities; validation was bypassed, dropping the scope",
            missing.join(", "),
            event.r#type.as_str(),
        );
        return None;
    }
    let mut by_priority: HashMap<i32, Vec<&str>> = HashMap::new();
    for template in members {
        by_priority
            .entry(template.priority.unwrap_or_default())
            .or_default()
            .push(template.name.as_str());
    }
    if let Some((priority, duplicated)) = by_priority.into_iter().find(|(_, names)| names.len() > 1)
    {
        warn!(
            "Trigger templates [{}] share priority {} on event {}; validation was bypassed, dropping the scope",
            duplicated.join(", "),
            priority,
            event.r#type.as_str(),
        );
        return None;
    }
    members
        .iter()
        .max_by_key(|t| t.priority.unwrap_or_default())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::EventType;
    use wf_types::trigger::TriggerDispatchMode;

    use crate::trigger::test_fixtures::{base_event, event_template, names};

    #[test]
    fn best_win_highest_explicit_priority_wins() {
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = Some(1);
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let event = base_event(EventType::NodeCompleted, "e1");
        let winners = arbitrate(&[low, high], &event, &TriggerRuntimeLimits::default());
        assert_eq!(names(&winners), vec!["high"]);
    }

    #[test]
    fn best_win_without_explicit_priority_is_dropped() {
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = None;
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(arbitrate(&[low, high], &event, &TriggerRuntimeLimits::default()).is_empty());
    }

    #[test]
    fn best_win_with_duplicate_priority_is_dropped() {
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = Some(5);
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(arbitrate(&[low, high], &event, &TriggerRuntimeLimits::default()).is_empty());
    }

    #[test]
    fn unique_scope_with_several_matches_is_dropped() {
        let candidates = vec![
            event_template("a", "NODE_COMPLETED", 0),
            event_template("b", "NODE_COMPLETED", 0),
        ];
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(arbitrate(&candidates, &event, &TriggerRuntimeLimits::default()).is_empty());
    }

    /// Global `NODE_COMPLETED` subscription (no `event_name` discriminator).
    fn global_event() -> TriggerTemplate {
        event_template("global", "NODE_COMPLETED", 0)
    }

    /// `NODE_COMPLETED` subscription restricted to one event name, hence a
    /// competition scope of its own.
    fn named_event(event_name: &str) -> TriggerTemplate {
        let mut template = event_template("named", "NODE_COMPLETED", 0);
        template.condition.as_mut().expect("condition").event_name = Some(event_name.to_string());
        template
    }

    #[test]
    fn disjoint_scopes_without_burst_cap_drop_every_winner() {
        // Two competition scopes (different `event_name` discriminators) that
        // both answer the same event: that can only happen when validation was
        // bypassed, so nothing runs.
        let candidates = vec![global_event(), named_event("on_done")];
        let event = BaseEvent {
            event_name: Some("on_done".to_string()),
            ..base_event(EventType::NodeCompleted, "e1")
        };
        assert!(arbitrate(&candidates, &event, &TriggerRuntimeLimits::default()).is_empty());
    }

    #[test]
    fn disjoint_scopes_with_burst_cap_keep_deterministic_winner() {
        let candidates = vec![global_event(), named_event("on_done")];
        let event = BaseEvent {
            event_name: Some("on_done".to_string()),
            ..base_event(EventType::NodeCompleted, "e1")
        };
        let limits = TriggerRuntimeLimits {
            dispatch_burst_limit: Some(1),
            ..TriggerRuntimeLimits::default()
        };
        let winners = arbitrate(&candidates, &event, &limits);
        assert_eq!(names(&winners), vec!["global"]);
    }

    #[test]
    fn disjoint_scopes_with_keep_first_policy_keep_deterministic_winner() {
        use wf_types::trigger::MultiScopePolicy;
        let candidates = vec![named_event("on_done"), global_event()];
        let event = BaseEvent {
            event_name: Some("on_done".to_string()),
            ..base_event(EventType::NodeCompleted, "e1")
        };
        let limits = TriggerRuntimeLimits {
            multi_scope_policy: Some(MultiScopePolicy::KeepFirst),
            ..TriggerRuntimeLimits::default()
        };
        let winners = arbitrate(&candidates, &event, &limits);
        assert_eq!(names(&winners), vec!["global"]);
    }

    #[test]
    fn single_candidate_scope_wins() {
        let candidates = vec![event_template("only", "NODE_COMPLETED", 0)];
        let winners = arbitrate(
            &candidates,
            &base_event(EventType::NodeCompleted, "e1"),
            &TriggerRuntimeLimits::default(),
        );
        assert_eq!(names(&winners), vec!["only"]);
    }
}
