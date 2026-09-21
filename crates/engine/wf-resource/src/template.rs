//! Unified template render engine.
//!
//! Single rendering entry for every templateable prompt text (system-prompt
//! templates, tool-visibility announcements, the discoverable metadata
//! block, the `general` tool description). Consumers only know template ids;
//! the data source (predefined / custom / hot-reloaded) is transparent.
//!
//! Template content uses the `{{name}}` placeholder syntax. One pseudo
//! variable is resolved by the engine rather than substituted verbatim:
//!
//! - `{{fragments}}`: the template's declared fragment list, composed in
//!   declaration order (a missing fragment fails the render).
//!
//! Any other `{{name}}` placeholder (including the removed
//! `{{tool_descriptions}}`) is substituted from the render variables.
//! Declared default values fill missing entries first; a still-missing
//! required variable fails the render, while an optional one stays verbatim
//! with an unresolved-placeholder warning.
//!
//! Two-stage pipeline: this engine only resolves double-brace placeholders.
//! Post-render injection anchors (single-brace uppercase markers resolved by
//! the tool and skill layers) pass through untouched and are never treated
//! as template variables.
//!
//! When no template is registered for an id, the built-in default text is
//! used with a fallback metric so unconfigured deployments stay visible.
//! Unknown ids render to `None`.

use std::collections::HashMap;

use wf_core::registry::Registry;
use wf_metrics::TemplateMetricsCollector;
use wf_types::Template;

use crate::predefined::tool_visibility::{
    ACTIVATION_CONTENT, ACTIVATION_TEMPLATE_ID, BLOCK_CONTENT, BLOCK_TEMPLATE_ID,
    DISCOVERABLE_METADATA_CONTENT, DISCOVERABLE_METADATA_TEMPLATE_ID, GENERAL_DESCRIPTION_CONTENT,
    GENERAL_DESCRIPTION_TEMPLATE_ID,
};
use crate::registry::ResourceRegistries;

/// Options for one display-template render.
/// Display use only and always lenient: approval hints, visibility
/// fallbacks, previews. Unresolved placeholders stay verbatim with a
/// warning and a metric. Model-bound callers must use
/// [`render_template_for_model`] so declared shapes are checked and
/// residuals fail closed. The flat entry refuses templates declaring
/// non-string shapes so shape errors cannot slip through stringified values.
#[derive(Debug, Clone, Default)]
pub struct TemplateRenderOptions {
    /// `{{name}}` placeholder values.
    pub variables: HashMap<String, String>,
}

/// Substitute `{{name}}` placeholders in a single left-to-right pass.
/// Display-layer flat matching only for hint and preview text; dotted
/// paths need the structured model-bound entry and never split here.
/// Built-in fallback texts used when a template is not configured. The texts
/// reference the predefined visibility constants so injected and fallback
/// deployments are indistinguishable by construction.
pub fn builtin_default(id: &str) -> Option<&'static str> {
    match id {
        ACTIVATION_TEMPLATE_ID => Some(ACTIVATION_CONTENT),
        BLOCK_TEMPLATE_ID => Some(BLOCK_CONTENT),
        DISCOVERABLE_METADATA_TEMPLATE_ID => Some(DISCOVERABLE_METADATA_CONTENT),
        GENERAL_DESCRIPTION_TEMPLATE_ID => Some(GENERAL_DESCRIPTION_CONTENT),
        _ => None,
    }
}

/// Render a display template with the given options. Configured templates
/// win; unconfigured ids fall back to [`builtin_default`]. Unknown ids
/// return `None`. Display use only without shape checks and always lenient
/// on residuals; model-bound text must use [`render_template_for_model`].
/// Templates declaring non-string shapes are refused here so callers cannot
/// bypass typed rendering through stringified values.
pub fn render_template(
    regs: &ResourceRegistries,
    id: &str,
    opts: &TemplateRenderOptions,
) -> Option<String> {
    render_template_with_metrics(regs, id, opts, None)
}

/// Explicit lenient display entry. Records duration and errors into the
/// template collector. Absent collectors add zero overhead. Missing
/// required variables, shape refusals, missing fragments and unknown ids
/// fail as `None`; residual placeholders stay verbatim with a warning so
/// previews never interrupt. Model-bound callers use the strict structured
/// entry so residuals fail closed.
pub fn render_template_for_display(
    regs: &ResourceRegistries,
    id: &str,
    opts: &TemplateRenderOptions,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    render_template_with_metrics(regs, id, opts, metrics)
}

/// Render a display template and record duration and unknown-id errors into
/// the template collector. Absent collectors add zero overhead. Missing
/// required variables fail the render as `None` plus a metric so partial
/// prompts never reach the model, while strict command execution keeps
/// returning `Result`. Display use only and always lenient on residuals;
/// model-bound callers use the strict entry so shape mismatches fail before
/// stringification. Templates declaring non-string shapes are refused here
/// for the same reason.
pub fn render_template_with_metrics(
    regs: &ResourceRegistries,
    id: &str,
    opts: &TemplateRenderOptions,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    let start = std::time::Instant::now();
    let template: Option<Template> = regs.templates.get(id).map(|t| t.as_ref().clone());
    let using_builtin_fallback = template.is_none();
    let content = template
        .as_ref()
        .map(|t| t.content.clone())
        .or_else(|| builtin_default(id).map(String::from));
    let Some(content) = content else {
        if let Some(metrics) = metrics {
            metrics.record_error(id, "unknown_template", &[]);
        }
        return None;
    };
    if using_builtin_fallback {
        if let Some(metrics) = metrics {
            metrics.record_error(id, "builtin_fallback", &[]);
        }
        tracing::warn!("template '{id}' is not registered; using built-in default text");
    }

    let mut rendered = content;

    // Pseudo variable (only meaningful for configured templates; the
    // built-in fallbacks carry no fragments).
    if let Some(ref template) = template {
        if let Some(offender) = flat_entry_typed_offender(regs, template) {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "variable_type_mismatch", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            tracing::warn!(
                "template '{id}' declares non-string variable '{offender}'; use the structured JSON entry so shapes are checked before stringification"
            );
            return None;
        }
        let mut effective_variables = opts.variables.clone();
        apply_default_values(regs, template, &mut effective_variables);
        if let Some(missing) = missing_required_variables(regs, template, &effective_variables) {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "missing_required_variable", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            tracing::warn!("template '{id}' missing required variable '{missing}'");
            return None;
        }
        if fragments_ignored(template, &rendered) {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "fragments_ignored", &[]);
            }
            tracing::warn!(
                "template '{id}' declares fragments but the content never uses '{{fragments}}'"
            );
        }
        let Some(resolved) = resolve_fragments(regs, &rendered, template, &effective_variables)
        else {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "missing_fragment", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            return None;
        };
        rendered = resolved;
        let output = wf_common::template::apply_template_variables(&rendered, &effective_variables);
        if has_unresolved_placeholders(&output) {
            let kind = unresolved_error_kind(&output);
            if let Some(metrics) = metrics {
                metrics.record_error(id, kind, &[]);
            }
            tracing::warn!("template '{id}' rendered with unresolved placeholders ({kind})");
        }
        if let Some(metrics) = metrics {
            metrics.record_render_complete(id, start.elapsed().as_millis() as f64, true, &[]);
        }
        return Some(output);
    }

    let output = wf_common::template::apply_template_variables(&rendered, &opts.variables);
    if has_unresolved_placeholders(&output) {
        let kind = unresolved_error_kind(&output);
        if let Some(metrics) = metrics {
            metrics.record_error(id, kind, &[]);
        }
        tracing::warn!("template '{id}' rendered with unresolved placeholders ({kind})");
    }
    if let Some(metrics) = metrics {
        metrics.record_render_complete(id, start.elapsed().as_millis() as f64, true, &[]);
    }
    Some(output)
}

fn unresolved_error_kind(output: &str) -> &'static str {
    if !wf_common::template::find_malformed_template_spans(output).is_empty() {
        return "malformed_template";
    }
    if wf_common::template::has_empty_template_placeholder(output) {
        return "empty_placeholder";
    }
    for name in wf_common::template::extract_placeholder_names(output) {
        if wf_common::template::validate_template_path(&name).is_some() {
            return "invalid_path";
        }
    }
    "missing_variable"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResidualPolicy {
    Lenient,
    Deny,
}

/// Render a template from structured values, checking each provided value
/// against its declared shape before stringification. Explicit lenient
/// display entry: residuals stay verbatim with a warning. Model-bound
/// callers must use [`render_template_for_model`]. A shape mismatch fails
/// the render as `None` so type errors surface at render time rather than
/// inside the model. Undeclared values pass through as display text.
/// Dotted placeholders resolve as paths against the structured table,
/// matching script and hook engines; flat names match exactly first.
/// Explicit lenient structured display entry. Residuals stay verbatim;
/// use only for previews, never for model requests.
pub fn render_template_with_json_for_display(
    regs: &ResourceRegistries,
    id: &str,
    variables: &HashMap<String, serde_json::Value>,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    render_template_with_json_inner(regs, id, variables, ResidualPolicy::Lenient, metrics)
}

pub fn render_template_with_json_variables(
    regs: &ResourceRegistries,
    id: &str,
    variables: &HashMap<String, serde_json::Value>,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    render_template_with_json_for_display(regs, id, variables, metrics)
}

fn render_template_with_json_inner(
    regs: &ResourceRegistries,
    id: &str,
    variables: &HashMap<String, serde_json::Value>,
    policy: ResidualPolicy,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    let template: Option<Template> = regs.templates.get(id).map(|t| t.as_ref().clone());
    if let Some(ref template) = template {
        let definitions = collect_variable_definitions(regs, template);
        for (name, value) in variables {
            if let Some(definition) = definitions.get(name) {
                if !wf_types::template::variable_value_matches(&definition.r#type, value) {
                    if let Some(metrics) = metrics {
                        metrics.record_error(id, "variable_type_mismatch", &[]);
                    }
                    tracing::warn!(
                        "template '{id}' variable '{name}' does not match declared shape"
                    );
                    return None;
                }
            }
        }
        for (name, definition) in &definitions {
            if !name.contains('.') {
                continue;
            }
            if let Some(resolved) = wf_common::template::resolve_value_path(name, variables) {
                if !wf_types::template::variable_value_matches(&definition.r#type, &resolved) {
                    if let Some(metrics) = metrics {
                        metrics.record_error(id, "variable_type_mismatch", &[]);
                    }
                    tracing::warn!(
                        "template '{id}' variable '{name}' does not match declared shape"
                    );
                    return None;
                }
            }
        }
    }
    let mut structured: HashMap<String, serde_json::Value> = variables.clone();
    if let Some(ref template) = template {
        let definitions = collect_variable_definitions(regs, template);
        for (name, definition) in definitions {
            if name.contains('.') {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(entry) = structured.entry(name) {
                if let Some(default_value) = definition.default_value {
                    entry.insert(default_value);
                }
            }
        }
    }
    let string_variables: HashMap<String, String> = structured
        .iter()
        .map(|(k, v)| (k.clone(), wf_common::template::value_to_display_string(v)))
        .collect();
    render_template_with_structured_context(
        regs,
        id,
        &string_variables,
        &structured,
        policy,
        metrics,
    )
}

/// Model-bound strict render with residual placeholders denied. Single home
/// for text sent to the model. Missing, illegal, empty and malformed
/// residuals fail as `None` with a classified metric.
pub fn render_template_for_model(
    regs: &ResourceRegistries,
    id: &str,
    variables: &HashMap<String, serde_json::Value>,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    render_template_with_json_inner(regs, id, variables, ResidualPolicy::Deny, metrics)
}

/// Render with both a flat display table and a structured path table.
/// Flat exact matches win; otherwise spans resolve as dotted paths.
/// Internal primary shared by the explicit lenient display and strict model
/// entries; the flat-only public entry stays for pure display callers such
/// as approval hints.
fn render_template_with_structured_context(
    regs: &ResourceRegistries,
    id: &str,
    flat: &HashMap<String, String>,
    structured: &HashMap<String, serde_json::Value>,
    policy: ResidualPolicy,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    let start = std::time::Instant::now();
    let template: Option<Template> = regs.templates.get(id).map(|t| t.as_ref().clone());
    let using_builtin_fallback = template.is_none();
    let content = template
        .as_ref()
        .map(|t| t.content.clone())
        .or_else(|| builtin_default(id).map(String::from));
    let Some(content) = content else {
        if let Some(metrics) = metrics {
            metrics.record_error(id, "unknown_template", &[]);
        }
        return None;
    };
    if using_builtin_fallback {
        if let Some(metrics) = metrics {
            metrics.record_error(id, "builtin_fallback", &[]);
        }
        tracing::warn!("template '{id}' is not registered; using built-in default text");
    }
    if let Some(ref template) = template {
        let mut effective_flat = flat.clone();
        let mut effective_structured = structured.clone();
        apply_default_values(regs, template, &mut effective_flat);
        apply_default_values_to_structured(regs, template, &mut effective_structured);
        for (k, v) in &effective_structured {
            effective_flat
                .entry(k.clone())
                .or_insert_with(|| wf_common::template::value_to_display_string(v));
        }
        if let Some(missing) = missing_required_variables_structured(
            regs,
            template,
            &effective_flat,
            &effective_structured,
        ) {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "missing_required_variable", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            tracing::warn!("template '{id}' missing required variable '{missing}'");
            return None;
        }
        if fragments_ignored(template, &content) {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "fragments_ignored", &[]);
            }
            tracing::warn!(
                "template '{id}' declares fragments but the content never uses '{{fragments}}'"
            );
        }
        let Some(resolved) = resolve_fragments_with_structured(
            regs,
            &content,
            template,
            &effective_flat,
            &effective_structured,
        ) else {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "missing_fragment", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            return None;
        };
        let output = wf_common::template::apply_template_variables_with_structured_context(
            &resolved,
            &effective_flat,
            &effective_structured,
        );
        if has_unresolved_placeholders(&output) {
            let kind = unresolved_error_kind(&output);
            if let Some(metrics) = metrics {
                metrics.record_error(id, kind, &[]);
            }
            if policy == ResidualPolicy::Deny {
                if let Some(metrics) = metrics {
                    metrics.record_render_complete(
                        id,
                        start.elapsed().as_millis() as f64,
                        false,
                        &[],
                    );
                }
                tracing::warn!("template '{id}' denied: residual {kind} remains");
                return None;
            }
            tracing::warn!("template '{id}' rendered with unresolved placeholders ({kind})");
        }
        if let Some(metrics) = metrics {
            metrics.record_render_complete(id, start.elapsed().as_millis() as f64, true, &[]);
        }
        return Some(output);
    }
    let output = wf_common::template::apply_template_variables_with_structured_context(
        &content, flat, structured,
    );
    if has_unresolved_placeholders(&output) {
        let kind = unresolved_error_kind(&output);
        if let Some(metrics) = metrics {
            metrics.record_error(id, kind, &[]);
        }
        if policy == ResidualPolicy::Deny {
            if let Some(metrics) = metrics {
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            tracing::warn!("template '{id}' denied: residual {kind} remains");
            return None;
        }
        tracing::warn!("template '{id}' rendered with unresolved placeholders ({kind})");
    }
    if let Some(metrics) = metrics {
        metrics.record_render_complete(id, start.elapsed().as_millis() as f64, true, &[]);
    }
    Some(output)
}

/// Collect variable declarations from a template and its fragments.
/// Template declarations win on name collision.
fn collect_variable_definitions(
    regs: &ResourceRegistries,
    template: &Template,
) -> HashMap<String, wf_types::TemplateVariableDefinition> {
    let mut map: HashMap<String, wf_types::TemplateVariableDefinition> = HashMap::new();
    if let Some(fragment_ids) = template.fragments.as_ref() {
        for fragment_id in fragment_ids {
            if let Some(fragment) = regs.fragments.get(fragment_id) {
                if let Some(vars) = fragment.variables.as_ref() {
                    for variable in vars {
                        map.entry(variable.name.clone())
                            .or_insert_with(|| variable.clone());
                    }
                }
            }
        }
    }
    if let Some(declared) = template.variables.as_ref() {
        for variable in declared {
            map.insert(variable.name.clone(), variable.clone());
        }
    }
    map
}

/// First variable declaring a non-string shape, if any. The flat display
/// entry carries only strings and cannot validate such shapes, so its
/// presence means the caller must use the structured JSON entry.
fn flat_entry_typed_offender(regs: &ResourceRegistries, template: &Template) -> Option<String> {
    let mut names: Vec<String> = collect_variable_definitions(regs, template)
        .into_iter()
        .filter(|(_, definition)| {
            !matches!(definition.r#type, wf_types::TemplateVariableType::String)
        })
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names.into_iter().next()
}

/// Fill missing variables from declared default values.
/// Presence wins: a declared default fills the slot even when its display
/// text is empty, so empty-string defaults satisfy required variables.
/// Dotted declarations never materialize as flat keys here; they resolve
/// through the structured table so flat exact matches cannot shadow paths.
fn apply_default_values(
    regs: &ResourceRegistries,
    template: &Template,
    variables: &mut HashMap<String, String>,
) {
    for (name, definition) in collect_variable_definitions(regs, template) {
        if name.contains('.') {
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = variables.entry(name) {
            if let Some(default_value) = definition.default_value.as_ref() {
                let text = wf_common::template::value_to_display_string(default_value);
                entry.insert(text);
            }
        }
    }
}

/// Fill missing structured roots from declared defaults.
/// Dotted declarations never create nested structure here; only root
/// names materialize as values.
fn apply_default_values_to_structured(
    regs: &ResourceRegistries,
    template: &Template,
    variables: &mut HashMap<String, serde_json::Value>,
) {
    for (name, definition) in collect_variable_definitions(regs, template) {
        if name.contains('.') {
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = variables.entry(name) {
            if let Some(default_value) = definition.default_value {
                entry.insert(default_value);
            }
        }
    }
}

/// First required variable still missing after defaults, if any.
fn missing_required_variables(
    regs: &ResourceRegistries,
    template: &Template,
    variables: &HashMap<String, String>,
) -> Option<String> {
    for (name, definition) in collect_variable_definitions(regs, template) {
        if definition.required && !variables.contains_key(&name) {
            return Some(name);
        }
    }
    None
}

/// Required check aware of dotted placeholders: a required root is
/// satisfied by a flat entry or a structured root; a required dotted path
/// is satisfied when the path resolves in either table.
fn missing_required_variables_structured(
    regs: &ResourceRegistries,
    template: &Template,
    flat: &HashMap<String, String>,
    structured: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    for (name, definition) in collect_variable_definitions(regs, template) {
        if !definition.required {
            continue;
        }
        if flat.contains_key(&name) {
            continue;
        }
        if structured.contains_key(&name) {
            continue;
        }
        if wf_common::template::validate_template_path(&name).is_none()
            && wf_common::template::resolve_value_path_ref(&name, structured).is_some()
        {
            continue;
        }
        return Some(name);
    }
    None
}

/// Whether rendered text still carries template syntax. Used only for
/// observability; rendering keeps the verbatim behavior. Delegates to the
/// shared foundation check so closed, empty, and unclosed spans agree.
fn has_unresolved_placeholders(rendered: &str) -> bool {
    wf_common::template::has_unresolved_placeholders(rendered)
}

/// Replace every `{{name}}` span whose trimmed placeholder name matches the
/// pseudo variable, tolerating surrounding whitespace exactly like the
/// foundation substitution scan. Returns the rewritten text plus whether
/// any span matched.
fn replace_pseudo_variable(content: &str, name: &str, replacement: &str) -> (String, bool) {
    let spans = wf_common::template::scan_template_spans(content);
    if spans.is_empty() {
        return (content.to_string(), false);
    }
    let mut rendered = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut matched = false;
    for span in spans {
        rendered.push_str(&content[cursor..span.start]);
        if !span.closed {
            rendered.push_str(&content[span.start..]);
            cursor = content.len();
            break;
        }
        if span.name == name {
            rendered.push_str(replacement);
            matched = true;
        } else {
            rendered.push_str(&content[span.start..span.end]);
        }
        cursor = span.end;
    }
    rendered.push_str(&content[cursor..]);
    (rendered, matched)
}

/// Whether a template declares fragments that the content never consumes.
/// The render keeps the historical ignore behavior, but callers deserve a
/// warning so a misconfigured list does not look like composition.
fn fragments_ignored(template: &Template, content: &str) -> bool {
    let Some(ids) = template.fragments.as_ref() else {
        return false;
    };
    if ids.is_empty() {
        return false;
    }
    !wf_common::template::extract_placeholder_names(content)
        .iter()
        .any(|name| name == "fragments")
}

/// Resolve the `{{fragments}}` pseudo variable by composing the template's
/// declared fragments (each with the render variables applied). A declared
/// fragment that is not registered fails the render so partial prompts
/// never reach the model. Spaced spellings resolve identically to the
/// foundation variable scan.
fn resolve_fragments(
    regs: &ResourceRegistries,
    content: &str,
    template: &Template,
    variables: &HashMap<String, String>,
) -> Option<String> {
    if !wf_common::template::extract_placeholder_names(content)
        .iter()
        .any(|name| name == "fragments")
    {
        return Some(content.to_string());
    }
    let Some(fragment_ids) = template.fragments.as_ref() else {
        return Some(replace_pseudo_variable(content, "fragments", "").0);
    };
    let mut parts = Vec::with_capacity(fragment_ids.len());
    let mut missing: Vec<&str> = Vec::new();
    for id in fragment_ids {
        match regs.fragments.get(id) {
            Some(fragment) => {
                parts.push(wf_common::template::apply_template_variables(
                    &fragment.content,
                    variables,
                ));
            }
            None => missing.push(id.as_str()),
        }
    }
    if !missing.is_empty() {
        tracing::warn!(
            "template '{}' references missing fragments: {}",
            template.id,
            missing.join(", ")
        );
        return None;
    }
    Some(replace_pseudo_variable(content, "fragments", &parts.join("\n\n")).0)
}

/// Fragment composition against both flat and structured tables.
/// Each fragment renders path-aware so dotted variables work inside
/// fragments exactly like in the template body.
fn resolve_fragments_with_structured(
    regs: &ResourceRegistries,
    content: &str,
    template: &Template,
    flat: &HashMap<String, String>,
    structured: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    if !wf_common::template::extract_placeholder_names(content)
        .iter()
        .any(|name| name == "fragments")
    {
        return Some(content.to_string());
    }
    let Some(fragment_ids) = template.fragments.as_ref() else {
        return Some(replace_pseudo_variable(content, "fragments", "").0);
    };
    let mut parts = Vec::with_capacity(fragment_ids.len());
    let mut missing: Vec<&str> = Vec::new();
    for id in fragment_ids {
        match regs.fragments.get(id) {
            Some(fragment) => {
                parts.push(
                    wf_common::template::apply_template_variables_with_structured_context(
                        &fragment.content,
                        flat,
                        structured,
                    ),
                );
            }
            None => missing.push(id.as_str()),
        }
    }
    if !missing.is_empty() {
        tracing::warn!(
            "template '{}' references missing fragments: {}",
            template.id,
            missing.join(", ")
        );
        return None;
    }
    Some(replace_pseudo_variable(content, "fragments", &parts.join("\n\n")).0)
}

/// Render the built-in visibility text for a template id with variables
/// applied. Single source for the fallback wording used when no registry
/// is injected.
pub fn render_builtin_visibility_fallback(
    template_id: &str,
    variables: &HashMap<String, String>,
) -> Option<String> {
    builtin_default(template_id)
        .map(|content| wf_common::template::apply_template_variables(content, variables))
}

/// Render an activation/block announcement, falling back to the
/// caller-supplied fallback when the template is not registered.
pub fn render_visibility_message(
    regs: Option<&ResourceRegistries>,
    template_id: &str,
    fallback: &str,
    variables: &HashMap<String, String>,
) -> String {
    render_visibility_message_with_metrics(regs, template_id, fallback, variables, None)
}

pub fn render_visibility_message_with_metrics(
    regs: Option<&ResourceRegistries>,
    template_id: &str,
    fallback: &str,
    variables: &HashMap<String, String>,
    metrics: Option<&TemplateMetricsCollector>,
) -> String {
    let Some(regs) = regs else {
        return fallback.to_string();
    };
    let structured: HashMap<String, serde_json::Value> = variables
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    render_template_for_model(regs, template_id, &structured, metrics)
        .unwrap_or_else(|| fallback.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::MutableRegistry;

    fn regs_with_template(
        id: &str,
        content: &str,
        fragments: Option<Vec<String>>,
    ) -> ResourceRegistries {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                id.to_string(),
                std::sync::Arc::new(Template {
                    id: id.into(),
                    name: "test".into(),
                    description: None,
                    category: "test".into(),
                    content: content.into(),
                    variables: None,
                    fragments,
                }),
            )
            .unwrap();
        regs
    }

    #[test]
    fn fallback_defaults_render_variables() {
        let regs = ResourceRegistries::new();
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("tool_names".to_string(), "write_file".to_string())]),
            ..Default::default()
        };
        let text = render_template(&regs, ACTIVATION_TEMPLATE_ID, &opts).expect("default exists");
        assert!(text.contains("write_file"));
        assert!(!text.contains("{tool_names}"));

        assert!(render_template(&regs, "no-such-template", &Default::default()).is_none());
    }

    #[test]
    fn configured_templates_win_over_defaults() {
        let regs = regs_with_template(ACTIVATION_TEMPLATE_ID, "Custom: {{tool_names}}", None);
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("tool_names".to_string(), "shell".to_string())]),
            ..Default::default()
        };
        let text = render_template(&regs, ACTIVATION_TEMPLATE_ID, &opts).expect("configured");
        assert!(text.starts_with("Custom:"));
        assert!(text.contains("shell"));
    }

    #[test]
    fn fragments_pseudo_variable_with_declared_list() {
        let regs = ResourceRegistries::new();
        regs.fragments
            .register(
                "fragments.role.assistant".into(),
                std::sync::Arc::new(wf_types::SystemPromptFragment {
                    id: "fragments.role.assistant".into(),
                    category: "role".into(),
                    content: "You are a helpful assistant.".into(),
                    description: None,
                    variables: None,
                }),
            )
            .unwrap();
        regs.fragments
            .register(
                "fragments.constraint.general".into(),
                std::sync::Arc::new(wf_types::SystemPromptFragment {
                    id: "fragments.constraint.general".into(),
                    category: "constraint".into(),
                    content: "Be kind.".into(),
                    description: None,
                    variables: None,
                }),
            )
            .unwrap();
        regs.templates
            .register(
                "system.test".into(),
                std::sync::Arc::new(Template {
                    id: "system.test".into(),
                    name: "test".into(),
                    description: None,
                    category: "system".into(),
                    content: "HEADER\n{{fragments}}".into(),
                    variables: None,
                    fragments: Some(vec![
                        "fragments.role.assistant".into(),
                        "fragments.constraint.general".into(),
                    ]),
                }),
            )
            .unwrap();

        let opts = TemplateRenderOptions::default();
        let text = render_template(&regs, "system.test", &opts).expect("rendered");
        assert!(text.contains("HEADER"));
        assert!(text.contains("You are a helpful assistant."));
        assert!(text.contains("Be kind."));
        assert!(!text.contains("{{fragments}}"));

        // A fragment-less template renders the placeholder empty.
        regs.templates
            .register(
                "system.empty".into(),
                std::sync::Arc::new(Template {
                    id: "system.empty".into(),
                    name: "empty".into(),
                    description: None,
                    category: "system".into(),
                    content: "{{fragments}}".into(),
                    variables: None,
                    fragments: None,
                }),
            )
            .unwrap();
        assert_eq!(
            render_template(&regs, "system.empty", &opts).expect("rendered"),
            ""
        );
    }

    #[test]
    fn removed_tool_descriptions_placeholder_stays_verbatim() {
        let regs = regs_with_template("system.deprecated-tools", "A {{tool_descriptions}} B", None);
        let text = render_template(&regs, "system.deprecated-tools", &Default::default())
            .expect("rendered");
        assert_eq!(text, "A {{tool_descriptions}} B");

        let spaced = regs_with_template("system.spaced-tools", "A {{ tool_descriptions }} B", None);
        let spaced_text =
            render_template(&spaced, "system.spaced-tools", &Default::default()).expect("rendered");
        assert_eq!(spaced_text, "A {{ tool_descriptions }} B");
    }

    #[test]
    fn reload_replaces_templates() {
        let regs = regs_with_template(BLOCK_TEMPLATE_ID, "V1", None);
        let opts = TemplateRenderOptions::default();
        assert_eq!(
            render_template(&regs, BLOCK_TEMPLATE_ID, &opts).unwrap(),
            "V1"
        );

        // Hot reload: replace the template under the same id.
        regs.templates.unregister(BLOCK_TEMPLATE_ID);
        regs.templates
            .register(
                BLOCK_TEMPLATE_ID.to_string(),
                std::sync::Arc::new(Template {
                    id: BLOCK_TEMPLATE_ID.into(),
                    name: "block".into(),
                    description: None,
                    category: "tool-visibility".into(),
                    content: "V2".into(),
                    variables: None,
                    fragments: None,
                }),
            )
            .unwrap();
        assert_eq!(
            render_template(&regs, BLOCK_TEMPLATE_ID, &opts).unwrap(),
            "V2"
        );
    }

    #[test]
    fn canonical_double_brace_syntax_renders() {
        let regs = regs_with_template("t.canonical", "Hi {{who}}!", None);
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("who".to_string(), "dev".to_string())]),
            ..Default::default()
        };
        assert_eq!(
            render_template(&regs, "t.canonical", &opts).unwrap(),
            "Hi dev!"
        );

        // Single braces are not placeholders and stay verbatim.
        let regs_single = regs_with_template("t.single", "Hi {who}!", None);
        assert_eq!(
            render_template(&regs_single, "t.single", &opts).unwrap(),
            "Hi {who}!"
        );

        // Unresolved placeholders are kept verbatim.
        let regs_unresolved = regs_with_template("t.unresolved", "Hi {{who}}!", None);
        assert_eq!(
            render_template(&regs_unresolved, "t.unresolved", &Default::default()).unwrap(),
            "Hi {{who}}!"
        );
    }

    #[test]
    fn pseudo_variables_tolerate_surrounding_whitespace() {
        let regs = ResourceRegistries::new();
        regs.fragments
            .register(
                "f.whitespace".into(),
                std::sync::Arc::new(wf_types::SystemPromptFragment {
                    id: "f.whitespace".into(),
                    category: "test".into(),
                    content: "fragment body".into(),
                    description: None,
                    variables: None,
                }),
            )
            .unwrap();
        regs.templates
            .register(
                "system.spaced".into(),
                std::sync::Arc::new(Template {
                    id: "system.spaced".into(),
                    name: "spaced".into(),
                    description: None,
                    category: "system".into(),
                    content: "HEADER\n{{ fragments }}".into(),
                    variables: None,
                    fragments: Some(vec!["f.whitespace".into()]),
                }),
            )
            .unwrap();
        let text = render_template(&regs, "system.spaced", &Default::default()).expect("rendered");
        assert!(text.contains("fragment body"));
        assert!(!text.contains("{{"));
    }

    #[test]
    fn visibility_message_falls_back_without_registries() {
        let fallback = "fallback text";
        let msg = render_visibility_message(
            None,
            ACTIVATION_TEMPLATE_ID,
            fallback,
            &HashMap::from([("tool_names".to_string(), "shell".to_string())]),
        );
        assert_eq!(msg, "fallback text");
    }

    #[test]
    fn builtin_fragments_render_without_placeholders() {
        for fragment in crate::predefined::fragments::builtin_fragments() {
            assert!(
                wf_common::template::extract_placeholder_names(&fragment.content).is_empty(),
                "builtin fragment '{}' must not leak placeholders",
                fragment.id
            );
        }
    }

    #[test]
    fn missing_required_variable_fails_closed() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.required".into(),
                std::sync::Arc::new(Template {
                    id: "system.required".into(),
                    name: "required".into(),
                    description: None,
                    category: "system".into(),
                    content: "Hello {{who}}".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "who".into(),
                        r#type: wf_types::TemplateVariableType::String,
                        required: true,
                        description: None,
                        default_value: None,
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        assert!(render_template(&regs, "system.required", &Default::default()).is_none());
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("who".to_string(), "dev".to_string())]),
            ..Default::default()
        };
        assert_eq!(
            render_template(&regs, "system.required", &opts).expect("rendered"),
            "Hello dev"
        );
    }

    #[test]
    fn declared_default_value_fills_missing_variable() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.defaulted".into(),
                std::sync::Arc::new(Template {
                    id: "system.defaulted".into(),
                    name: "defaulted".into(),
                    description: None,
                    category: "system".into(),
                    content: "Hello {{who}}".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "who".into(),
                        r#type: wf_types::TemplateVariableType::String,
                        required: true,
                        description: None,
                        default_value: Some(serde_json::Value::String("fallback".into())),
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let text =
            render_template(&regs, "system.defaulted", &Default::default()).expect("default fills");
        assert_eq!(text, "Hello fallback");
    }

    #[test]
    fn explicit_entries_separate_lenient_and_strict() {
        let regs = regs_with_template("t.deny", "Hi {{who}}!", None);
        assert_eq!(
            render_template(&regs, "t.deny", &Default::default()).unwrap(),
            "Hi {{who}}!"
        );
        let empty: HashMap<String, serde_json::Value> = HashMap::new();
        assert!(render_template_for_model(&regs, "t.deny", &empty, None).is_none());
        let filled = HashMap::from([("who".to_string(), serde_json::json!("dev"))]);
        assert_eq!(
            render_template_for_model(&regs, "t.deny", &filled, None).unwrap(),
            "Hi dev!"
        );
    }

    #[test]
    fn typed_variables_reject_shape_mismatch() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.typed".into(),
                std::sync::Arc::new(Template {
                    id: "system.typed".into(),
                    name: "typed".into(),
                    description: None,
                    category: "system".into(),
                    content: "Count {{count}}".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "count".into(),
                        r#type: wf_types::TemplateVariableType::Number,
                        required: true,
                        description: None,
                        default_value: None,
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let bad = HashMap::from([("count".to_string(), serde_json::json!("not-a-number"))]);
        assert!(render_template_with_json_variables(&regs, "system.typed", &bad, None).is_none());
        let good = HashMap::from([("count".to_string(), serde_json::json!(7))]);
        let text = render_template_with_json_variables(&regs, "system.typed", &good, None)
            .expect("typed render");
        assert_eq!(text, "Count 7");
    }

    #[test]
    fn empty_string_default_counts_as_present() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.empty-default".into(),
                std::sync::Arc::new(Template {
                    id: "system.empty-default".into(),
                    name: "empty-default".into(),
                    description: None,
                    category: "system".into(),
                    content: "Hello {{who}}!".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "who".into(),
                        r#type: wf_types::TemplateVariableType::String,
                        required: true,
                        description: None,
                        default_value: Some(serde_json::Value::String(String::new())),
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let text = render_template(&regs, "system.empty-default", &Default::default())
            .expect("empty default fills");
        assert_eq!(text, "Hello !");
    }

    #[test]
    fn structured_variables_resolve_dotted_paths() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.dotted".into(),
                std::sync::Arc::new(Template {
                    id: "system.dotted".into(),
                    name: "dotted".into(),
                    description: None,
                    category: "system".into(),
                    content: "Hi {{user.name}}!".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "user".into(),
                        r#type: wf_types::TemplateVariableType::Object,
                        required: true,
                        description: None,
                        default_value: None,
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let vars = HashMap::from([("user".to_string(), serde_json::json!({"name": "ada"}))]);
        let text = render_template_with_json_variables(&regs, "system.dotted", &vars, None)
            .expect("dotted resolves");
        assert_eq!(text, "Hi ada!");
        let missing: HashMap<String, serde_json::Value> = HashMap::new();
        assert!(render_template_for_model(&regs, "system.dotted", &missing, None).is_none());
    }

    #[test]
    fn unclosed_placeholders_count_as_unresolved() {
        let regs = regs_with_template("t.unclosed", "Hi {{who", None);
        assert!(has_unresolved_placeholders("Hi {{who"));
        assert_eq!(unresolved_error_kind("Hi {{who"), "malformed_template");
        assert!(render_template(&regs, "t.unclosed", &Default::default()).is_some());
        let empty: HashMap<String, serde_json::Value> = HashMap::new();
        assert!(render_template_for_model(&regs, "t.unclosed", &empty, None).is_none());
    }

    #[test]
    fn flat_entry_refuses_typed_declarations() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.typed-flat".into(),
                std::sync::Arc::new(Template {
                    id: "system.typed-flat".into(),
                    name: "typed-flat".into(),
                    description: None,
                    category: "system".into(),
                    content: "Count {{count}}".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "count".into(),
                        r#type: wf_types::TemplateVariableType::Number,
                        required: true,
                        description: None,
                        default_value: None,
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("count".to_string(), "7".to_string())]),
            ..Default::default()
        };
        assert!(render_template(&regs, "system.typed-flat", &opts).is_none());
        let typed = HashMap::from([("count".to_string(), serde_json::json!(7))]);
        assert_eq!(
            render_template_with_json_variables(&regs, "system.typed-flat", &typed, None)
                .expect("typed entry renders"),
            "Count 7"
        );
    }

    #[test]
    fn dotted_default_does_not_shadow_structured_path() {
        let regs = ResourceRegistries::new();
        regs.templates
            .register(
                "system.dotted-default".into(),
                std::sync::Arc::new(Template {
                    id: "system.dotted-default".into(),
                    name: "dotted-default".into(),
                    description: None,
                    category: "system".into(),
                    content: "Hi {{user.name}}!".into(),
                    variables: Some(vec![wf_types::TemplateVariableDefinition {
                        name: "user.name".into(),
                        r#type: wf_types::TemplateVariableType::String,
                        required: false,
                        description: None,
                        default_value: Some(serde_json::Value::String("fallback".into())),
                    }]),
                    fragments: None,
                }),
            )
            .unwrap();
        let text = render_template(&regs, "system.dotted-default", &Default::default())
            .expect("flat renders leniently");
        assert_eq!(text, "Hi {{user.name}}!");
    }
}
