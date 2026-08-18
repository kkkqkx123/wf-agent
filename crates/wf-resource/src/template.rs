//! Unified template render engine.
//!
//! Single rendering entry for every templateable prompt text (system-prompt
//! templates, tool-visibility announcements, the discoverable metadata
//! block, the `general` tool description). Consumers only know template ids;
//! the data source (predefined / custom / hot-reloaded) is transparent.
//!
//! Template content uses the `{{name}}` placeholder syntax; legacy
//! single-brace `{name}` templates keep rendering for
//! backwards compatibility. Two pseudo variables are resolved by the engine
//! rather than substituted verbatim:
//!
//! - `{{fragments}}`: the template's declared fragment list, composed in
//!   declaration order (missing fragments are skipped);
//! - `{{tool_descriptions}}`: the optional tool description set rendered in
//!   the requested format (empty when none is supplied).
//!
//! When no template is registered for an id, the built-in default text is
//! used, so unconfigured deployments keep the previous behavior. Unknown
//! ids render to `None`.

use std::collections::HashMap;

use wf_core::registry::Registry;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::Template;

use crate::predefined::render::{render_tool_descriptions, ToolFormat};
use crate::predefined::tool_visibility::{
    ACTIVATION_TEMPLATE_ID, BLOCK_TEMPLATE_ID, DISCOVERABLE_METADATA_TEMPLATE_ID,
    GENERAL_DESCRIPTION_TEMPLATE_ID,
};
use crate::registry::ResourceRegistries;

/// Options for one template render.
#[derive(Debug, Clone, Default)]
pub struct TemplateRenderOptions<'a> {
    /// `{{name}}` placeholder values (legacy `{name}` also rendered).
    pub variables: HashMap<String, String>,
    /// Tool descriptions resolved for the `{{tool_descriptions}}` pseudo
    /// variable; the placeholder stays empty when absent.
    pub tool_descriptions: Option<&'a [ToolDescriptionData]>,
    /// Format for the tool description pseudo variable (defaults to
    /// [`ToolFormat::Xml`]).
    pub tool_format: Option<ToolFormat>,
}

/// Substitute `{{name}}` (canonical) and legacy `{name}` placeholders.
///
/// Double-brace placeholders are replaced first so a canonical token never
/// leaves stray braces behind; unresolvable placeholders are kept verbatim.
/// Shared by the template engine, the fragment composer and call sites that
/// pre-render fragment content.
pub fn apply_template_variables(
    content: &str,
    variables: &HashMap<String, String>,
) -> String {
    let mut rendered = content.to_string();
    for (key, value) in variables {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
        rendered = rendered.replace(&format!("{{{}}}", key), value);
    }
    rendered
}

/// Built-in fallback texts used when a template is not configured. The
/// texts match the predefined templates word for word so injected and
/// fallback deployments are indistinguishable.
pub fn builtin_default(id: &str) -> Option<&'static str> {
    match id {
        ACTIVATION_TEMPLATE_ID => Some(
            "[Tool Activation] The following tools are now available: {{tool_names}}.\nYou can call them directly or via the general tool.",
        ),
        BLOCK_TEMPLATE_ID => Some("The following tools are now unavailable:\n{{tool_names}}"),
        DISCOVERABLE_METADATA_TEMPLATE_ID => Some(
            "Discoverable tools:\n{{tool_list}}\nInvoke them via the general tool.",
        ),
        GENERAL_DESCRIPTION_TEMPLATE_ID => Some(
            "Invoke tools whose schemas are not directly exposed. The request body is a JSON object {\"tool\": \"tool_name\", \"parameters\": {...}} passed as the `request` parameter, e.g.:\n{{invoke_example}}\nThe inner tool is interpreted and executed server-side.",
        ),
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
    let template: Option<Template> = regs.templates.get(id).map(|t| t.as_ref().clone());
    let content = template
        .as_ref()
        .map(|t| t.content.clone())
        .or_else(|| builtin_default(id).map(String::from))?;

    let mut rendered = content;

    // Pseudo variables (only meaningful for configured templates; the
    // built-in fallbacks carry no fragments and no tool sections).
    if let Some(ref template) = template {
        rendered = resolve_fragments(regs, &rendered, template, &opts.variables);
        rendered = resolve_tool_descriptions(&rendered, opts);
    }

    Some(apply_template_variables(&rendered, &opts.variables))
}

/// Resolve the `{{fragments}}` pseudo variable (legacy `{fragments}`
/// accepted) by composing the template's declared fragments (each with the
/// render variables applied).
fn resolve_fragments(
    regs: &ResourceRegistries,
    content: &str,
    template: &Template,
    variables: &HashMap<String, String>,
) -> String {
    let has_pseudo =
        content.contains("{{fragments}}") || content.contains("{fragments}");
    if !has_pseudo {
        return content.to_string();
    }
    let composed = match template.fragments.as_ref() {
        None => String::new(),
        Some(fragment_ids) => fragment_ids
            .iter()
            .filter_map(|id| regs.fragments.get(id))
            .map(|fragment| apply_template_variables(&fragment.content, variables))
            .collect::<Vec<_>>()
            .join("\n\n"),
    };
    content
        .replace("{{fragments}}", &composed)
        .replace("{fragments}", &composed)
}

/// Resolve the `{{tool_descriptions}}` pseudo variable (legacy form
/// accepted; empty when no tool description set is supplied).
fn resolve_tool_descriptions(content: &str, opts: &TemplateRenderOptions) -> String {
    let has_pseudo = content.contains("{{tool_descriptions}}")
        || content.contains("{tool_descriptions}");
    if !has_pseudo {
        return content.to_string();
    }
    let rendered = match opts.tool_descriptions {
        Some(tools) if !tools.is_empty() => {
            let format = opts.tool_format.unwrap_or(ToolFormat::Xml);
            render_tool_descriptions(tools, format)
        }
        _ => String::new(),
    };
    content
        .replace("{{tool_descriptions}}", &rendered)
        .replace("{tool_descriptions}", &rendered)
}

/// Render an activation/block announcement, falling back to the legacy
/// text when neither the template nor the fallback applies.
pub fn render_visibility_message(
    regs: Option<&ResourceRegistries>,
    template_id: &str,
    fallback: &str,
    variables: &HashMap<String, String>,
) -> String {
    let Some(regs) = regs else {
        return fallback.to_string();
    };
    let opts = TemplateRenderOptions {
        variables: variables.clone(),
        ..Default::default()
    };
    render_template(regs, template_id, &opts).unwrap_or_else(|| fallback.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::MutableRegistry;

    fn regs_with_template(id: &str, content: &str, fragments: Option<Vec<String>>) -> ResourceRegistries {
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
        let regs = regs_with_template(ACTIVATION_TEMPLATE_ID, "Custom: {tool_names}", None);
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
                    content: "HEADER\n{fragments}".into(),
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
        assert!(!text.contains("{fragments}"));

        // A fragment-less template renders the placeholder empty.
        regs.templates
            .register(
                "system.empty".into(),
                std::sync::Arc::new(Template {
                    id: "system.empty".into(),
                    name: "empty".into(),
                    description: None,
                    category: "system".into(),
                    content: "{fragments}".into(),
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
    fn tool_descriptions_pseudo_variable_renders_supplied_tools() {
        let regs = regs_with_template("system.tools", "{tool_descriptions}", None);
        let tools = vec![ToolDescriptionData {
            id: "web_search".into(),
            r#type: "function".into(),
            category: None,
            description: "Search the web".into(),
            parameters: Vec::new(),
            tips: None,
            examples: None,
        }];
        let opts = TemplateRenderOptions {
            variables: HashMap::new(),
            tool_descriptions: Some(&tools),
            tool_format: None,
        };
        let text = render_template(&regs, "system.tools", &opts).expect("rendered");
        assert!(text.contains("web_search"));
        assert!(!text.contains("{tool_descriptions}"));

        // Without tool descriptions the placeholder resolves to empty.
        let empty = render_template(&regs, "system.tools", &Default::default()).expect("rendered");
        assert_eq!(empty, "");
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
    fn canonical_double_brace_syntax_renders_and_legacy_still_works() {
        // Canonical `{{name}}` in a configured template.
        let regs = regs_with_template("t.canonical", "Hi {{who}}!", None);
        let opts = TemplateRenderOptions {
            variables: HashMap::from([("who".to_string(), "dev".to_string())]),
            ..Default::default()
        };
        assert_eq!(
            render_template(&regs, "t.canonical", &opts).unwrap(),
            "Hi dev!"
        );

        // Legacy single-brace templates keep rendering.
        let regs_legacy = regs_with_template("t.legacy", "Hi {who}!", None);
        assert_eq!(
            render_template(&regs_legacy, "t.legacy", &opts).unwrap(),
            "Hi dev!"
        );

        // Unresolved placeholders are kept verbatim.
        let regs_unresolved = regs_with_template("t.unresolved", "Hi {{who}}!", None);
        assert_eq!(
            render_template(&regs_unresolved, "t.unresolved", &Default::default()).unwrap(),
            "Hi {{who}}!"
        );
    }

    #[test]
    fn visibility_message_falls_back_without_registries() {
        let fallback = "legacy text";
        let msg = render_visibility_message(
            None,
            ACTIVATION_TEMPLATE_ID,
            fallback,
            &HashMap::from([("tool_names".to_string(), "shell".to_string())]),
        );
        assert_eq!(msg, "legacy text");
    }
}
