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
//! `{{tool_descriptions}}`) is substituted from the render variables and
//! otherwise kept verbatim with an unresolved-placeholder warning.
//!
//! Two-stage pipeline: this engine only resolves double-brace placeholders.
//! Post-render injection anchors (single-brace uppercase markers resolved by
//! the tool and skill layers) pass through untouched and are never treated
//! as template variables.
//!
//! When no template is registered for an id, the built-in default text is
//! used, so unconfigured deployments keep the previous behavior. Unknown
//! ids render to `None`.

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

/// Options for one template render.
#[derive(Debug, Clone, Default)]
pub struct TemplateRenderOptions {
    /// `{{name}}` placeholder values.
    pub variables: HashMap<String, String>,
}

/// Substitute `{{name}}` placeholders in a single left-to-right pass;
/// unresolvable placeholders are kept verbatim. Single braces are never
/// treated as placeholders, so literal JSON such as `{"tool": "x"}`
/// passes through untouched. Values are inserted as opaque text and never
/// rescanned, so a value containing placeholder shapes cannot expand
/// again. Shared by the template engine and call
/// sites that pre-render fragment content. Delegates to the shared
/// foundation implementation so resource rendering and message injection
/// stay byte-identical.
pub fn apply_template_variables(content: &str, variables: &HashMap<String, String>) -> String {
    wf_common::template::apply_template_variables(content, variables)
}

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

/// Render a template with the given options. Configured templates win;
/// unconfigured ids fall back to [`builtin_default`]. Unknown ids return
/// `None`.
pub fn render_template(
    regs: &ResourceRegistries,
    id: &str,
    opts: &TemplateRenderOptions,
) -> Option<String> {
    render_template_with_metrics(regs, id, opts, None)
}

/// Render a template and record duration and unknown-id errors into the
/// template collector. Absent collectors add zero overhead. This is
/// best-effort text rendering: failures surface as `None` plus a metric,
/// while strict command execution keeps returning `Result`.
pub fn render_template_with_metrics(
    regs: &ResourceRegistries,
    id: &str,
    opts: &TemplateRenderOptions,
    metrics: Option<&TemplateMetricsCollector>,
) -> Option<String> {
    let start = std::time::Instant::now();
    let template: Option<Template> = regs.templates.get(id).map(|t| t.as_ref().clone());
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

    let mut rendered = content;

    // Pseudo variable (only meaningful for configured templates; the
    // built-in fallbacks carry no fragments).
    if let Some(ref template) = template {
        // Required variables are the union of the template declaration and
        // its fragments' declarations: fragment content renders with the
        // same variable map, so a missing fragment variable degrades the
        // same way as a missing template variable.
        {
            let mut required_names: Vec<String> = Vec::new();
            if let Some(declared) = template.variables.as_ref() {
                for variable in declared.iter().filter(|v| v.required) {
                    if !required_names.iter().any(|name| name == &variable.name) {
                        required_names.push(variable.name.clone());
                    }
                }
            }
            if let Some(fragment_ids) = template.fragments.as_ref() {
                for fragment_id in fragment_ids {
                    if let Some(fragment) = regs.fragments.get(fragment_id) {
                        if let Some(vars) = fragment.variables.as_ref() {
                            for variable in vars.iter().filter(|v| v.required) {
                                if !required_names.iter().any(|name| name == &variable.name) {
                                    required_names.push(variable.name.clone());
                                }
                            }
                        }
                    }
                }
            }
            for name in &required_names {
                if !opts.variables.contains_key(name) {
                    if let Some(metrics) = metrics {
                        metrics.record_error(id, "missing_required_variable", &[]);
                    }
                    tracing::warn!("template '{id}' missing required variable '{name}'");
                }
            }
        }
        let Some(resolved) = resolve_fragments(regs, &rendered, template, &opts.variables) else {
            if let Some(metrics) = metrics {
                metrics.record_error(id, "missing_fragment", &[]);
                metrics.record_render_complete(id, start.elapsed().as_millis() as f64, false, &[]);
            }
            return None;
        };
        rendered = resolved;
    }

    let output = apply_template_variables(&rendered, &opts.variables);
    if let Some(metrics) = metrics {
        if has_unresolved_placeholders(&output) {
            metrics.record_error(id, "unresolved_placeholder", &[]);
            tracing::warn!("template '{id}' rendered with unresolved placeholders");
        }
        metrics.record_render_complete(id, start.elapsed().as_millis() as f64, true, &[]);
    } else if has_unresolved_placeholders(&output) {
        tracing::warn!("template '{id}' rendered with unresolved placeholders");
    }
    Some(output)
}

/// Whether rendered text still carries `{{name}}` placeholders. Used only
/// for observability; rendering keeps the verbatim behavior. Delegates to
/// the config-layer scanner so validation and observability share one scan.
fn has_unresolved_placeholders(rendered: &str) -> bool {
    !wf_common::template::extract_placeholder_names(rendered).is_empty()
}

/// Replace every `{{name}}` span whose trimmed placeholder name matches the
/// pseudo variable, tolerating surrounding whitespace exactly like the
/// foundation substitution scan. Returns the rewritten text plus whether
/// any span matched.
fn replace_pseudo_variable(content: &str, name: &str, replacement: &str) -> (String, bool) {
    let mut rendered = String::with_capacity(content.len());
    let mut rest = content;
    let mut matched = false;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        if after[..end].trim() == name {
            rendered.push_str(&rest[..start]);
            rendered.push_str(replacement);
            matched = true;
        } else {
            rendered.push_str(&rest[..start + 2 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    rendered.push_str(rest);
    (rendered, matched)
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
                parts.push(apply_template_variables(&fragment.content, variables));
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
    builtin_default(template_id).map(|content| apply_template_variables(content, variables))
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
    let opts = TemplateRenderOptions {
        variables: variables.clone(),
        ..Default::default()
    };
    render_template_with_metrics(regs, template_id, &opts, metrics)
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

        let spaced =
            regs_with_template("system.spaced-tools", "A {{ tool_descriptions }} B", None);
        let spaced_text = render_template(&spaced, "system.spaced-tools", &Default::default())
            .expect("rendered");
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
}
