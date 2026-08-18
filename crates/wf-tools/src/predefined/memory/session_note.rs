//! Definition and stateful instance of the session_note tool.
//!
//! Single persistent memory tool: notes are
//! stored in a SQLite database (default `<workspace>/data/session-notes.db`)
//! shared across executions, with create / list / get / update / delete /
//! search operations.

use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::memory::note_store::{NewNote, NoteEntry, NotePatch, SessionNoteStore};
use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};
use crate::registry::ToolRegistry;

static OPERATIONS: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: Some(&["create", "list", "get", "update", "delete", "search"]),
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: None,
    maximum: None,
    min_items: None,
    max_items: None,
    items: None,
};

/// Description is a single static string; concatenation at build time keeps
/// the definition `'static` without requiring a long literal.
const SESSION_NOTE_DESCRIPTION: &str = "Manage persistent session notes. Notes survive across \
executions in a SQLite database (default <workspace>/data/session-notes.db). Operations: create, \
list (category filter, newest first), get, update, delete and search.";

pub static SESSION_NOTE: ToolDefinition = ToolDefinition {
    id: "session_note",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "memory",
    tags: &["note", "persistent"],
    description: SESSION_NOTE_DESCRIPTION,
    parameters: &[
        ToolParameter {
            name: "operation",
            r#type: "string",
            required: true,
            description: "The note operation to run",
            default_json: None,
            constraints: Some(&OPERATIONS),
        },
        ToolParameter {
            name: "content",
            r#type: "string",
            required: false,
            description: "The note content (create / update)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "category",
            r#type: "string",
            required: false,
            description: "Note category (create / update; list filter)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "summary",
            r#type: "string",
            required: false,
            description: "Optional one-line summary of the note (create / update)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "note_id",
            r#type: "string",
            required: false,
            description: "Target note id (get / update / delete)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "query",
            r#type: "string",
            required: false,
            description: "Search term over note content and summary (search)",
            default_json: None,
            constraints: None,
        },
    ],
    tips: None,
    examples: Some(&[
        "session_note(\"create\", content=\"User prefers dark mode\", category=\"preferences\")",
        "session_note(\"list\", category=\"preferences\")",
    ]),
};

struct SessionNoteInstance {
    store: OnceLock<Arc<dyn SessionNoteStore>>,
    db_path: String,
    execution_id: String,
}

impl SessionNoteInstance {
    fn store(&self) -> &Arc<dyn SessionNoteStore> {
        // Lazy open (`getOrCreateStorage` singleton semantics): the
        // database file is only created on first use.
        self.store
            .get_or_init(|| super::note_store::open_store(&self.db_path))
    }
}

fn token_count(text: &str) -> i64 {
    // Tokens are estimated as ~1 token per 4 characters.
    (text.chars().count() as i64 / 4).max(1)
}

fn entry_json(entry: &NoteEntry) -> Value {
    json!({
        "id": entry.id,
        "category": entry.category,
        "content": entry.content,
        "summary": entry.summary,
        "token_count": entry.token_count,
        "timestamp": entry.timestamp,
        "created_at": entry.created_at,
        "updated_at": entry.updated_at,
    })
}

fn required_note_id(params: &Value) -> Option<&str> {
    params
        .get("note_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn optional_str(params: &Value, name: &str) -> Option<String> {
    params
        .get(name)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

impl StatefulInstance for SessionNoteInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'operation' parameter".into())
            })?;

        match operation {
            "create" => self.create(params),
            "list" => self.list(params),
            "get" => self.get(params),
            "update" => self.update(params),
            "delete" => self.delete(params),
            "search" => self.search(params),
            other => Err(ToolError::ValidationFailed(format!(
                "Unsupported session_note operation: {other}"
            ))),
        }
    }
}

impl SessionNoteInstance {
    fn create(&self, params: &Value) -> ToolResult<Value> {
        let content = optional_str(params, "content").ok_or_else(|| {
            ToolError::ValidationFailed("session_note create requires 'content'".into())
        })?;
        let category = optional_str(params, "category").unwrap_or_else(|| "general".to_string());
        let summary = optional_str(params, "summary").unwrap_or_default();
        let tokens = token_count(&format!("{content} {summary}"));

        let entry = self.store().save(
            &self.execution_id,
            NewNote {
                category,
                content: content.clone(),
                summary,
                token_count: tokens,
                timestamp: wf_common::time::timestamp_to_iso(wf_common::time::now()),
            },
        );

        Ok(json!({
            "created": true,
            "note": entry_json(&entry),
        }))
    }

    fn list(&self, params: &Value) -> ToolResult<Value> {
        let category = optional_str(params, "category");
        let notes = self.store().list(&self.execution_id, category.as_deref());
        let items: Vec<Value> = notes.iter().map(entry_json).collect();
        Ok(json!({
            "notes": items,
            "total": notes.len(),
        }))
    }

    fn get(&self, params: &Value) -> ToolResult<Value> {
        let Some(note_id) = required_note_id(params) else {
            return Err(ToolError::ValidationFailed(
                "session_note get requires 'note_id'".into(),
            ));
        };
        match self.store().get(&self.execution_id, note_id) {
            Some(entry) => Ok(json!({ "found": true, "note": entry_json(&entry) })),
            None => Ok(json!({ "found": false })),
        }
    }

    fn update(&self, params: &Value) -> ToolResult<Value> {
        let Some(note_id) = required_note_id(params) else {
            return Err(ToolError::ValidationFailed(
                "session_note update requires 'note_id'".into(),
            ));
        };
        let patch = NotePatch {
            category: optional_str(params, "category"),
            content: optional_str(params, "content"),
            summary: optional_str(params, "summary"),
            token_count: params
                .get("content")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(token_count),
            timestamp: None,
        };
        match self.store().update(&self.execution_id, note_id, patch) {
            Some(entry) => Ok(json!({ "updated": true, "note": entry_json(&entry) })),
            None => Ok(json!({ "updated": false })),
        }
    }

    fn delete(&self, params: &Value) -> ToolResult<Value> {
        let Some(note_id) = required_note_id(params) else {
            return Err(ToolError::ValidationFailed(
                "session_note delete requires 'note_id'".into(),
            ));
        };
        Ok(json!({
            "deleted": self.store().delete(&self.execution_id, note_id),
        }))
    }

    fn search(&self, params: &Value) -> ToolResult<Value> {
        let Some(query) = optional_str(params, "query") else {
            return Err(ToolError::ValidationFailed(
                "session_note search requires 'query'".into(),
            ));
        };
        let notes = self.store().search(&self.execution_id, &query);
        let items: Vec<Value> = notes.iter().map(entry_json).collect();
        Ok(json!({
            "notes": items,
            "total": notes.len(),
        }))
    }
}

/// Register the session_note stateful factory into the registry. The store
/// is shared by every execution; `db_path` defaults to
/// `<workspace>/data/session-notes.db`.
pub(crate) fn register(registry: &ToolRegistry, db_path: Option<&str>) -> ToolResult<()> {
    let db_path = db_path.unwrap_or("data/session-notes.db").to_string();
    registry.register_stateful_factory(
        "session_note",
        Arc::new(move |execution_id| {
            Box::new(SessionNoteInstance {
                store: OnceLock::new(),
                db_path: db_path.clone(),
                execution_id: execution_id.to_string(),
            })
        }),
    );
    Ok(())
}
