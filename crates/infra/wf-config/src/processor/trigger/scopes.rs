use super::*;

/// One violated scope: every template in the scope plus the subset that
/// belongs to the incoming batch, plus the message naming the scope, the
/// templates involved, and the way out.
pub struct TriggerScopeReport {
    pub names: Vec<String>,
    pub incoming_names: Vec<String>,
    pub message: String,
}

/// Validate a whole set of trigger templates at load time.
///
/// Runs the single-template validation for every template, then checks the
/// competition scopes shared by the set: the default unique dispatch rejects
/// a second subscriber of one scope, while the explicit best-win dispatch
/// requires every subscriber to declare a distinct priority. Returns the
/// first violation found.
pub fn validate_trigger_set(templates: &[TriggerTemplate]) -> ConfigResult<()> {
    for template in templates {
        validate_trigger_template(template)?;
    }
    if let Some(report) = check_trigger_scopes(&[], templates).into_iter().next() {
        return Err(ConfigError::Validation(report.message));
    }
    Ok(())
}

/// Unified production entry for registering `incoming` against `existing`.
///
/// Single source for the two-step shape (every incoming template must pass
/// [`validate_trigger_template`], then the merged set must pass
/// [`check_trigger_scopes`]). Returns the validated incoming templates plus
/// one report per violated scope; callers reject the incoming members of a
/// violated scope and warn on scopes with no incoming member. Test-only
/// callers keep using the fine-grained functions directly.
pub fn validate_trigger_registration(
    existing: &[TriggerTemplate],
    incoming: &[TriggerTemplate],
) -> (ConfigResult<Vec<TriggerTemplate>>, Vec<TriggerScopeReport>) {
    let mut valid = Vec::new();
    let mut first_error: Option<ConfigError> = None;
    for template in incoming {
        if let Err(e) = validate_trigger_template(template) {
            if first_error.is_none() {
                first_error = Some(e);
            }
            continue;
        }
        valid.push(template.clone());
    }
    if let Some(e) = first_error {
        return (Err(e), Vec::new());
    }
    let reports = check_trigger_scopes(existing, &valid);
    (Ok(valid), reports)
}

/// Check the competition scopes of a merged template set and report one
/// entry per violated scope.
///
/// `existing` holds the templates already registered, `incoming` the batch
/// about to be added. Same-name incoming entries replace the existing entry
/// of the same name before grouping. Every violated scope is reported;
/// callers reject the incoming members of a violated scope and warn on
/// violated scopes with no incoming member.
pub fn check_trigger_scopes(
    existing: &[TriggerTemplate],
    incoming: &[TriggerTemplate],
) -> Vec<TriggerScopeReport> {
    use std::collections::HashSet;
    use wf_types::trigger::{check_scope_group, scope_groups, violation_message, TriggerScopeKey};

    let mut merged: Vec<TriggerTemplate> = Vec::new();
    let replaced: HashSet<&str> = incoming.iter().map(|t| t.name.as_str()).collect();
    merged.extend(
        existing
            .iter()
            .filter(|t| !replaced.contains(t.name.as_str()))
            .cloned(),
    );
    merged.extend(incoming.iter().cloned());

    let incoming_names: HashSet<&str> = incoming.iter().map(|t| t.name.as_str()).collect();
    let mut reports = Vec::new();
    for group in scope_groups(&merged) {
        let refs: Vec<&TriggerTemplate> = group.iter().map(|&i| &merged[i]).collect();
        let Some(first) = refs.first() else {
            continue;
        };
        let Some(condition) = first.condition.as_ref() else {
            continue;
        };
        let key = {
            let mut hook: Option<String> = None;
            let mut first_hook = true;
            for r in &refs {
                let h = r
                    .condition
                    .as_ref()
                    .and_then(wf_types::trigger::hook_type_dimension);
                if first_hook {
                    hook = h;
                    first_hook = false;
                } else if hook != h {
                    hook = None;
                    break;
                }
            }
            TriggerScopeKey {
                event_type: condition.event_type.clone(),
                event_name: condition.event_name.clone(),
                hook_type: hook,
            }
        };
        for violation in check_scope_group(&refs) {
            let names: Vec<String> = refs.iter().map(|t| t.name.clone()).collect();
            let involved: Vec<String> = refs
                .iter()
                .filter(|t| incoming_names.contains(t.name.as_str()))
                .map(|t| t.name.clone())
                .collect();
            reports.push(TriggerScopeReport {
                names,
                incoming_names: involved,
                message: violation_message(&key, &violation),
            });
        }
    }
    reports
}
