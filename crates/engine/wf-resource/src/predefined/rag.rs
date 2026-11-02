//! Retrieval context and citation templates.
//!
//! The retrieval kernel produces numbered context plus a citation list
//! (see `rag-core` citation helpers); these templates decide the final
//! layout. Two render modes share one variable shape: display text for
//! humans and strict text for the model. Truncation stays with the caller
//! through `excerpt_chars` so templates never guess length budgets.

use std::collections::HashMap;

use wf_types::Template;

use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;
use crate::template::{render_template_for_model, render_template_with_json_variables};

/// Model-bound retrieval context template id.
pub const RAG_CONTEXT_MODEL_TEMPLATE_ID: &str = "rag.context.model";
/// Display retrieval context template id.
pub const RAG_CONTEXT_DISPLAY_TEMPLATE_ID: &str = "rag.context.display";
/// Citation line template id.
pub const RAG_CITATION_TEMPLATE_ID: &str = "rag.citation";
/// No-hit answer template id.
pub const RAG_NO_HIT_TEMPLATE_ID: &str = "rag.no_hit";

pub const RAG_CONTEXT_MODEL_CONTENT: &str =
    "Retrieved context for the question:\n{{context}}\n\nQuestion: {{query}}\nAnswer using the numbered passages above and cite them as [n]. When no passage supports a claim, say so instead of citing.";
pub const RAG_CONTEXT_DISPLAY_CONTENT: &str =
    "Retrieved {{hit_count}} passage(s) for \"{{query}}\":\n{{context}}";
pub const RAG_CITATION_CONTENT: &str = "[{{index}}] {{source}} (score {{score}}): {{excerpt}}";
pub const RAG_NO_HIT_CONTENT: &str =
    "No indexed passages in namespace '{{namespace}}' matched the query '{{query}}'. {{hint}}";

fn template(
    id: &str,
    name: &str,
    description: &str,
    content: &str,
    variables: Vec<(&str, bool)>,
) -> Template {
    Template {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        category: "rag".to_string(),
        content: content.to_string(),
        variables: Some(
            variables
                .into_iter()
                .map(|(name, required)| wf_types::TemplateVariableDefinition {
                    name: name.to_string(),
                    r#type: wf_types::TemplateVariableType::String,
                    properties: None,
                    element_type: None,
                    required,
                    description: None,
                    default_value: None,
                })
                .collect(),
        ),
        fragments: None,
    }
}

/// All retrieval templates in registration order.
pub fn rag_templates() -> Vec<Template> {
    vec![
        template(
            RAG_CONTEXT_MODEL_TEMPLATE_ID,
            "RAG Model Context",
            "Numbered retrieval context assembled for the model",
            RAG_CONTEXT_MODEL_CONTENT,
            vec![("context", true), ("query", true)],
        ),
        template(
            RAG_CONTEXT_DISPLAY_TEMPLATE_ID,
            "RAG Display Context",
            "Numbered retrieval context rendered for humans",
            RAG_CONTEXT_DISPLAY_CONTENT,
            vec![("context", true), ("query", true), ("hit_count", true)],
        ),
        template(
            RAG_CITATION_TEMPLATE_ID,
            "RAG Citation Line",
            "Single citation line layout",
            RAG_CITATION_CONTENT,
            vec![
                ("index", true),
                ("source", true),
                ("score", false),
                ("excerpt", true),
            ],
        ),
        template(
            RAG_NO_HIT_TEMPLATE_ID,
            "RAG No-Hit Answer",
            "Answer shape when retrieval returns nothing",
            RAG_NO_HIT_CONTENT,
            vec![("namespace", true), ("query", true), ("hint", false)],
        ),
    ]
}

/// Builds the shared variable table for context templates from hits.
pub fn build_context_variables(
    hits: &[rag_core::RetrievedChunk],
    query: &str,
    excerpt_chars: usize,
) -> HashMap<String, serde_json::Value> {
    let (context, _) = rag_core::render_context(hits, excerpt_chars);
    HashMap::from([
        ("context".to_string(), serde_json::Value::String(context)),
        (
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        ),
        (
            "hit_count".to_string(),
            serde_json::Value::String(hits.len().to_string()),
        ),
    ])
}

/// Formats one citation through the citation template, falling back to the
/// kernel layout when the template is not registered.
pub fn format_citation(regs: &ResourceRegistries, citation: &rag_core::Citation) -> String {
    let variables = HashMap::from([
        (
            "index".to_string(),
            serde_json::Value::String(citation.index.to_string()),
        ),
        (
            "source".to_string(),
            serde_json::Value::String(citation.source.clone()),
        ),
        (
            "score".to_string(),
            serde_json::Value::String(format!("{:.3}", citation.score)),
        ),
        (
            "excerpt".to_string(),
            serde_json::Value::String(citation.excerpt.clone()),
        ),
    ]);
    render_template_with_json_variables(regs, RAG_CITATION_TEMPLATE_ID, &variables, None)
        .unwrap_or_else(|| {
            format!(
                "[{}] {}: {}",
                citation.index, citation.source, citation.excerpt
            )
        })
}

/// Renders model-bound retrieval context, denying residual placeholders.
pub fn render_model_context(
    regs: &ResourceRegistries,
    hits: &[rag_core::RetrievedChunk],
    query: &str,
    excerpt_chars: usize,
) -> Option<String> {
    let variables = build_context_variables(hits, query, excerpt_chars);
    render_template_for_model(regs, RAG_CONTEXT_MODEL_TEMPLATE_ID, &variables, None)
}

/// Renders display retrieval context, keeping residuals verbatim.
pub fn render_display_context(
    regs: &ResourceRegistries,
    hits: &[rag_core::RetrievedChunk],
    query: &str,
    excerpt_chars: usize,
) -> Option<String> {
    use crate::template::TemplateRenderOptions;
    let (context, _) = rag_core::render_context(hits, excerpt_chars);
    let mut flat = HashMap::new();
    flat.insert("context".to_string(), context);
    flat.insert("query".to_string(), query.to_string());
    flat.insert("hit_count".to_string(), hits.len().to_string());
    crate::template::render_template(
        regs,
        RAG_CONTEXT_DISPLAY_TEMPLATE_ID,
        &TemplateRenderOptions { variables: flat },
    )
}

/// Renders the no-hit answer shape.
pub fn render_no_hit_answer(
    regs: &ResourceRegistries,
    query: &str,
    namespace: &str,
    hint: &str,
    strict: bool,
) -> Option<String> {
    let variables = HashMap::from([
        (
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        ),
        (
            "namespace".to_string(),
            serde_json::Value::String(namespace.to_string()),
        ),
        (
            "hint".to_string(),
            serde_json::Value::String(hint.to_string()),
        ),
    ]);
    if strict {
        render_template_for_model(regs, RAG_NO_HIT_TEMPLATE_ID, &variables, None)
    } else {
        render_template_with_json_variables(regs, RAG_NO_HIT_TEMPLATE_ID, &variables, None)
    }
}

/// Registers retrieval templates.
pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for template in rag_templates() {
        total.merge(register_template(regs, template, opts.skip_if_exists));
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::Registry;

    fn hit(id: &str, source: &str) -> rag_core::RetrievedChunk {
        rag_core::RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: source.to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content of {id}"),
            score: 0.9,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    fn regs() -> ResourceRegistries {
        let regs = ResourceRegistries::new();
        register(&regs, &RegisterOptions::default());
        regs
    }

    #[test]
    fn templates_register_with_expected_ids() {
        let regs = regs();
        for id in [
            RAG_CONTEXT_MODEL_TEMPLATE_ID,
            RAG_CONTEXT_DISPLAY_TEMPLATE_ID,
            RAG_CITATION_TEMPLATE_ID,
            RAG_NO_HIT_TEMPLATE_ID,
        ] {
            assert!(regs.templates.has(id), "template '{id}' missing");
        }
    }

    #[test]
    fn model_and_display_context_render() {
        let regs = regs();
        let hits = vec![hit("doc.md#0", "doc.md")];
        let model = render_model_context(&regs, &hits, "q", 200).expect("model");
        assert!(model.contains("[1] doc.md"));
        assert!(model.contains('q'));
        let display = render_display_context(&regs, &hits, "q", 200).expect("display");
        assert!(display.contains('1'));
    }

    #[test]
    fn citation_formats_and_no_hit_renders() {
        let regs = regs();
        let (_, citations) = rag_core::render_context(&[hit("a", "doc.md")], 100);
        let line = format_citation(&regs, &citations[0]);
        assert!(line.contains("[1]"));
        assert!(line.contains("doc.md"));
        let answer = render_no_hit_answer(&regs, "q", "kb", "hint", true).expect("no-hit");
        assert!(answer.contains("kb"));
    }

    #[test]
    fn truncation_is_caller_controlled() {
        let regs = regs();
        let hits = vec![hit("a", "doc.md")];
        let short = render_display_context(&regs, &hits, "q", 4).expect("short");
        let long = render_display_context(&regs, &hits, "q", 400).expect("long");
        assert!(short.len() <= long.len());
    }
}
