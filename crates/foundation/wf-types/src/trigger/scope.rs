//! Competition scopes for trigger templates.
//!
//! Templates that can match the same event instance compete for one winner.
//! A scope groups templates by event type plus the secondary name
//! discriminator, further split by execution-prefix overlap: prefixes that
//! never match the same execution id belong to different scopes and never
//! compete. Metadata and expression conditions are deliberately excluded
//! from the key. Proving two such conditions disjoint is undecidable in
//! general, so grouping stays conservative and orthogonal subscriptions
//! resolve through an explicit BestWin declaration with distinct
//! priorities instead of relying on inferred disjointness.

use std::collections::HashMap;

use super::template::{TriggerDispatchMode, TriggerTemplate};

/// Competition scope key: templates sharing it may match the same event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TriggerScopeKey {
    pub event_type: String,
    pub event_name: Option<String>,
}

/// Whether two execution-prefix filters can match the same execution id.
/// An absent prefix matches every execution and therefore overlaps with
/// anything; two present prefixes overlap when one contains the other.
pub fn prefixes_overlap(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, _) | (_, None) => true,
        (Some(x), Some(y)) => x == y || x.starts_with(y) || y.starts_with(x),
    }
}

/// Whether a template takes part in competition: enabled templates carrying
/// a matchable condition. Disabled or condition-less templates never match
/// and are excluded from every scope.
pub fn competes(template: &TriggerTemplate) -> bool {
    template.enabled.unwrap_or(true) && template.condition.is_some()
}

/// Group template positions into competition scopes.
///
/// Only competing templates are grouped; each returned group holds indices
/// into the input slice. Templates sharing event type and secondary name
/// are merged transitively on prefix overlap, so a global template and a
/// prefixed one always land in one scope while disjoint prefixes split.
pub fn scope_groups(templates: &[TriggerTemplate]) -> Vec<Vec<usize>> {
    let competing: Vec<usize> = templates
        .iter()
        .enumerate()
        .filter(|(_, t)| competes(t))
        .map(|(i, _)| i)
        .collect();
    let mut parent: HashMap<usize, usize> = competing.iter().map(|&i| (i, i)).collect();
    fn find(parent: &mut HashMap<usize, usize>, x: usize) -> usize {
        let root = *parent.get(&x).unwrap_or(&x);
        if root == x {
            return x;
        }
        let resolved = find(parent, root);
        parent.insert(x, resolved);
        resolved
    }
    let key_of = |t: &TriggerTemplate| {
        let condition = t
            .condition
            .as_ref()
            .expect("competing template has a condition");
        TriggerScopeKey {
            event_type: condition.event_type.clone(),
            event_name: condition.event_name.clone(),
        }
    };
    for (a_pos, &a) in competing.iter().enumerate() {
        for &b in &competing[a_pos + 1..] {
            if key_of(&templates[a]) != key_of(&templates[b]) {
                continue;
            }
            let pa = templates[a]
                .condition
                .as_ref()
                .and_then(|c| c.execution_prefix.as_deref());
            let pb = templates[b]
                .condition
                .as_ref()
                .and_then(|c| c.execution_prefix.as_deref());
            if prefixes_overlap(pa, pb) {
                let ra = find(&mut parent, a);
                let rb = find(&mut parent, b);
                if ra != rb {
                    parent.insert(ra, rb);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in competing {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    groups.into_values().collect()
}

/// Resolved dispatch mode of one scope plus the declarations behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedScopeMode {
    Unique,
    BestWin,
}

/// Resolve the dispatch mode of a scope from its member declarations.
/// Any BestWin declaration switches the scope; an explicit Unique mixed
/// with BestWin is reported with both sides so the caller can reject it.
pub fn resolve_scope_mode<'a>(
    group: &[&'a TriggerTemplate],
) -> Result<ResolvedScopeMode, (Vec<&'a TriggerTemplate>, Vec<&'a TriggerTemplate>)> {
    let mut best_win = Vec::new();
    let mut unique = Vec::new();
    for template in group {
        match template
            .dispatch_mode
            .unwrap_or(TriggerDispatchMode::Unique)
        {
            TriggerDispatchMode::BestWin => best_win.push(*template),
            TriggerDispatchMode::Unique => unique.push(*template),
        }
    }
    if !best_win.is_empty() && !unique.iter().any(|t| t.dispatch_mode.is_some()) {
        return Ok(ResolvedScopeMode::BestWin);
    }
    if !best_win.is_empty() {
        return Err((unique, best_win));
    }
    Ok(ResolvedScopeMode::Unique)
}

/// One load-time violation found inside a competition scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeViolation {
    /// More than one subscriber under the default Unique mode.
    UniqueMultiSubscriber { names: Vec<String> },
    /// BestWin subscriber without an explicit priority.
    MissingPriority { name: String },
    /// BestWin subscribers sharing one explicit priority value.
    DuplicatePriority { priority: i32, names: Vec<String> },
    /// Explicit Unique mixed with BestWin in one scope.
    ConflictingMode {
        unique_names: Vec<String>,
        best_win_names: Vec<String>,
    },
}

/// Check one scope group and report every violation it contains.
pub fn check_scope_group(group: &[&TriggerTemplate]) -> Vec<ScopeViolation> {
    if group.len() <= 1 {
        if let Some(only) = group.first() {
            if only.dispatch_mode == Some(TriggerDispatchMode::BestWin) && only.priority.is_none() {
                return vec![ScopeViolation::MissingPriority {
                    name: only.name.clone(),
                }];
            }
        }
        return Vec::new();
    }
    match resolve_scope_mode(group) {
        Err((unique, best_win)) => vec![ScopeViolation::ConflictingMode {
            unique_names: unique.iter().map(|t| t.name.clone()).collect(),
            best_win_names: best_win.iter().map(|t| t.name.clone()).collect(),
        }],
        Ok(ResolvedScopeMode::Unique) => vec![ScopeViolation::UniqueMultiSubscriber {
            names: group.iter().map(|t| t.name.clone()).collect(),
        }],
        Ok(ResolvedScopeMode::BestWin) => {
            let mut violations = Vec::new();
            for template in group {
                if template.priority.is_none() {
                    violations.push(ScopeViolation::MissingPriority {
                        name: template.name.clone(),
                    });
                }
            }
            let mut by_priority: HashMap<i32, Vec<String>> = HashMap::new();
            for template in group {
                if let Some(priority) = template.priority {
                    by_priority
                        .entry(priority)
                        .or_default()
                        .push(template.name.clone());
                }
            }
            for (priority, names) in &by_priority {
                if names.len() > 1 {
                    violations.push(ScopeViolation::DuplicatePriority {
                        priority: *priority,
                        names: names.clone(),
                    });
                }
            }
            violations
        }
    }
}

/// Render a violation as a configuration error message naming the scope,
/// the templates involved, and the way out.
pub fn violation_message(key: &TriggerScopeKey, violation: &ScopeViolation) -> String {
    let scope = match &key.event_name {
        Some(name) => format!("event '{}' name '{}'", key.event_type, name),
        None => format!("event '{}'", key.event_type),
    };
    match violation {
        ScopeViolation::UniqueMultiSubscriber { names } => format!(
            "multiple trigger templates [{}] subscribe to {} which defaults to unique dispatch; merge them into one template or declare best_win dispatch with distinct priorities",
            names.join(", "),
            scope,
        ),
        ScopeViolation::MissingPriority { name } => format!(
            "trigger '{}' on {} declares best_win dispatch but sets no explicit priority; set a priority that differs from every other subscriber of the same scope",
            name, scope,
        ),
        ScopeViolation::DuplicatePriority { priority, names } => format!(
            "trigger templates [{}] on {} share priority {}; subscribers of one scope must declare distinct priorities so the winner is explicit",
            names.join(", "),
            scope,
            priority,
        ),
        ScopeViolation::ConflictingMode {
            unique_names,
            best_win_names,
        } => format!(
            "trigger templates [{}] declare unique dispatch while [{}] on {} declare best_win; subscribers of one scope must agree on the dispatch mode",
            unique_names.join(", "),
            best_win_names.join(", "),
            scope,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::TriggerCondition;

    fn template(
        name: &str,
        event_type: &str,
        event_name: Option<&str>,
        prefix: Option<&str>,
        priority: Option<i32>,
        mode: Option<TriggerDispatchMode>,
    ) -> TriggerTemplate {
        TriggerTemplate {
            name: name.to_string(),
            description: None,
            condition: Some(TriggerCondition {
                event_type: event_type.to_string(),
                event_name: event_name.map(str::to_string),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: prefix.map(str::to_string),
            }),
            action: None,
            enabled: None,
            max_triggers: None,
            priority,
            dispatch_mode: mode,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn absent_prefix_overlaps_everything() {
        assert!(prefixes_overlap(None, None));
        assert!(prefixes_overlap(None, Some("agent-")));
        assert!(prefixes_overlap(Some("agent-"), None));
        assert!(prefixes_overlap(Some("agent-"), Some("agent-")));
        assert!(prefixes_overlap(Some("agent-"), Some("agent-a-")));
        assert!(prefixes_overlap(Some("agent-a-"), Some("agent-")));
        assert!(!prefixes_overlap(Some("agent-a-"), Some("agent-b-")));
    }

    #[test]
    fn disjoint_prefixes_split_scopes() {
        let templates = [
            template("a", "NODE_COMPLETED", None, Some("exec-a-"), None, None),
            template("b", "NODE_COMPLETED", None, Some("exec-b-"), None, None),
        ];
        assert_eq!(scope_groups(&templates).len(), 2);
    }

    #[test]
    fn global_template_merges_with_prefixed_one() {
        let templates = [
            template("a", "NODE_COMPLETED", None, None, None, None),
            template("b", "NODE_COMPLETED", None, Some("exec-a-"), None, None),
        ];
        let groups = scope_groups(&templates);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn event_name_splits_scopes() {
        let templates = [
            template("a", "NODE_CUSTOM_EVENT", Some("created"), None, None, None),
            template("b", "NODE_CUSTOM_EVENT", Some("updated"), None, None, None),
        ];
        assert_eq!(scope_groups(&templates).len(), 2);
    }

    #[test]
    fn disabled_and_condition_less_templates_are_excluded() {
        let mut templates = vec![
            template("a", "NODE_COMPLETED", None, None, None, None),
            template("b", "NODE_COMPLETED", None, None, None, None),
        ];
        templates[1].enabled = Some(false);
        assert_eq!(scope_groups(&templates).len(), 1);
        assert_eq!(scope_groups(&templates)[0], vec![0]);
    }

    #[test]
    fn unique_scope_with_two_subscribers_violates() {
        let templates = [
            template("a", "NODE_COMPLETED", None, None, None, None),
            template("b", "NODE_COMPLETED", None, None, None, None),
        ];
        let groups = scope_groups(&templates);
        assert_eq!(groups.len(), 1);
        let refs: Vec<&TriggerTemplate> = groups[0].iter().map(|&i| &templates[i]).collect();
        let violations = check_scope_group(&refs);
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            violations[0],
            ScopeViolation::UniqueMultiSubscriber { .. }
        ));
    }

    #[test]
    fn best_win_requires_explicit_distinct_priorities() {
        let templates = [
            template(
                "a",
                "NODE_COMPLETED",
                None,
                None,
                None,
                Some(TriggerDispatchMode::BestWin),
            ),
            template(
                "b",
                "NODE_COMPLETED",
                None,
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        let refs: Vec<&TriggerTemplate> = templates.iter().collect();
        let violations = check_scope_group(&refs);
        assert!(violations.iter().any(|v| matches!(
            v,
            ScopeViolation::MissingPriority { name } if name == "a"
        )));

        let templates = [
            template(
                "a",
                "NODE_COMPLETED",
                None,
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
            template(
                "b",
                "NODE_COMPLETED",
                None,
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        let refs: Vec<&TriggerTemplate> = templates.iter().collect();
        let violations = check_scope_group(&refs);
        assert!(violations
            .iter()
            .any(|v| matches!(v, ScopeViolation::DuplicatePriority { priority: 1, .. })));

        let templates = [
            template(
                "a",
                "NODE_COMPLETED",
                None,
                None,
                Some(2),
                Some(TriggerDispatchMode::BestWin),
            ),
            template(
                "b",
                "NODE_COMPLETED",
                None,
                None,
                Some(1),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        let refs: Vec<&TriggerTemplate> = templates.iter().collect();
        assert!(check_scope_group(&refs).is_empty());
    }

    #[test]
    fn conflicting_mode_declarations_violate() {
        let templates = [
            template(
                "a",
                "NODE_COMPLETED",
                None,
                None,
                Some(1),
                Some(TriggerDispatchMode::Unique),
            ),
            template(
                "b",
                "NODE_COMPLETED",
                None,
                None,
                Some(2),
                Some(TriggerDispatchMode::BestWin),
            ),
        ];
        let refs: Vec<&TriggerTemplate> = templates.iter().collect();
        let violations = check_scope_group(&refs);
        assert!(violations
            .iter()
            .any(|v| matches!(v, ScopeViolation::ConflictingMode { .. })));
    }
}
