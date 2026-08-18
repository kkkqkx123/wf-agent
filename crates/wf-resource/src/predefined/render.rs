use wf_types::tool_description::ToolDescriptionData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolFormat {
    Xml,
    Json,
    Raw,
    Compact,
    Schema,
}

pub fn render_tool_descriptions(tools: &[ToolDescriptionData], format: ToolFormat) -> String {
    match format {
        ToolFormat::Xml => render_xml(tools),
        ToolFormat::Json => render_json(tools),
        ToolFormat::Raw => render_raw(tools),
        ToolFormat::Compact => render_compact(tools),
        ToolFormat::Schema => render_schema(tools),
    }
}

fn render_xml(tools: &[ToolDescriptionData]) -> String {
    let mut parts = Vec::new();
    parts.push("<tools>".into());
    for t in tools {
        let mut entry = format!(
            "  <tool name=\"{}\">\n    <description>{}</description>\n    <parameters>",
            t.id, t.description
        );
        for p in &t.parameters {
            let req = if p.required { " [required]" } else { "" };
            entry.push_str(&format!(
                "\n      - {} ({}){}: {}",
                p.name, p.r#type, req, p.description
            ));
        }
        entry.push_str("\n    </parameters>\n  </tool>");
        parts.push(entry);
    }
    parts.push("</tools>".into());
    parts.join("\n")
}

fn render_json(tools: &[ToolDescriptionData]) -> String {
    let mut parts = Vec::new();
    parts.push('['.into());
    for (i, t) in tools.iter().enumerate() {
        let comma = if i < tools.len() - 1 { "," } else { "" };
        let json = serde_json::json!({
            "name": t.id,
            "description": t.description,
            "parameters": t.parameters.iter().map(|p| serde_json::json!({
                "name": p.name,
                "type": p.r#type,
                "required": p.required,
                "description": p.description,
            })).collect::<Vec<_>>(),
        });
        parts.push(format!(
            "  {}{}",
            serde_json::to_string_pretty(&json).unwrap_or_default(),
            comma
        ));
    }
    parts.push(']'.into());
    parts.join("\n")
}

fn render_raw(tools: &[ToolDescriptionData]) -> String {
    let mut parts = Vec::new();
    for t in tools {
        let mut entry = format!("## {}\n\n{}\n\nParameters:", t.id, t.description);
        for p in &t.parameters {
            let req = if p.required { " (required)" } else { "" };
            entry.push_str(&format!("\n- `{}` (`{}`{})", p.name, p.r#type, req));
        }
        if let Some(ref tips) = t.tips {
            entry.push_str("\n\nTips:");
            for tip in tips {
                entry.push_str(&format!("\n- {}", tip));
            }
        }
        parts.push(entry);
    }
    parts.join("\n---\n")
}

fn render_compact(tools: &[ToolDescriptionData]) -> String {
    let mut parts = Vec::new();
    for t in tools {
        let params: Vec<&str> = t.parameters.iter().map(|p| p.name.as_str()).collect();
        parts.push(format!(
            "{}: {} ({})",
            t.id,
            t.description,
            params.join(", ")
        ));
    }
    parts.join("\n")
}

fn render_schema(tools: &[ToolDescriptionData]) -> String {
    let schemas: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            let properties: serde_json::Value = t
                .parameters
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        serde_json::json!({
                            "type": p.r#type,
                            "description": p.description,
                        }),
                    )
                })
                .collect();
            serde_json::json!({
                "name": t.id,
                "description": t.description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                }
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({"tools": schemas})).unwrap_or_default()
}
