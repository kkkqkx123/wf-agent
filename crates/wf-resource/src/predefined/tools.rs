use wf_types::tool::{Tool as ToolDef, ToolMetadata, ToolParameterSchema, ToolProperty, ToolType};

use crate::registrar::{register_item, Options, Registries};
use crate::result::Summary;

fn prop(name: &str, type_: &str, required: bool, desc: &str) -> ToolProperty {
    ToolProperty {
        name: name.into(),
        value: serde_json::Value::Null,
        r#type: Some(type_.into()),
        required: Some(required),
        description: Some(desc.into()),
    }
}

fn obj_schema(props: Vec<(String, ToolProperty)>, required: Vec<String>) -> ToolParameterSchema {
    ToolParameterSchema {
        r#type: "object".into(),
        properties: props.into_iter().collect(),
        required,
        additional_properties: None,
    }
}

fn metadata(category: &str, tags: Vec<&str>) -> Option<ToolMetadata> {
    Some(ToolMetadata {
        category: Some(category.into()),
        tags: Some(tags.into_iter().map(String::from).collect()),
        documentation_url: None,
        custom_fields: None,
        risk_level: None,
        auto_approvable: None,
    })
}

macro_rules! tool {
    ($id:expr, $name:expr, $desc:expr, $type_:expr, $schema:expr, $meta:expr) => {
        ToolDef {
            id: $id.into(),
            name: $name.into(),
            description: $desc.into(),
            tool_type: $type_,
            parameters: Some($schema),
            metadata: $meta,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    };
}

pub fn builtin_tools() -> Vec<ToolDef> {
    vec![
        // ── Stateless: Filesystem ──────────────────────────
        tool!("read_file", "read_file", "Read the contents of a file at the given path. Supports line-range and offset-based slicing.",
            ToolType::Stateless,
            obj_schema(vec![
                ("path".into(), prop("path", "string", true, "The absolute path to the file to read")),
                ("offset".into(), prop("offset", "number", false, "Line number to start reading from (1-indexed)")),
                ("limit".into(), prop("limit", "number", false, "Maximum number of lines to read")),
            ], vec!["path".into()]),
            metadata("filesystem", vec!["read", "file"])),

        tool!("write_file", "write_file", "Write content to a file at the given path. Creates the file and any missing parent directories.",
            ToolType::Stateless,
            obj_schema(vec![
                ("path".into(), prop("path", "string", true, "The absolute path to write the file")),
                ("content".into(), prop("content", "string", true, "The content to write to the file")),
            ], vec!["path".into(), "content".into()]),
            metadata("filesystem", vec!["write", "file"])),

        tool!("edit_file", "edit_file", "Perform an exact string replacement in a file. Replaces the first occurrence of old_string with new_string.",
            ToolType::Stateless,
            obj_schema(vec![
                ("file_path".into(), prop("file_path", "string", true, "The absolute path to the file to edit")),
                ("old_string".into(), prop("old_string", "string", true, "The exact text to search for")),
                ("new_string".into(), prop("new_string", "string", true, "The replacement text")),
            ], vec!["file_path".into(), "old_string".into(), "new_string".into()]),
            metadata("filesystem", vec!["edit", "file"])),

        tool!("apply_patch", "apply_patch", "Apply a unified diff patch to the filesystem. Parses and applies patch hunks to specified files.",
            ToolType::Stateless,
            obj_schema(vec![
                ("patch".into(), prop("patch", "string", true, "The patch content in unified diff format")),
            ], vec!["patch".into()]),
            metadata("filesystem", vec!["patch", "diff"])),

        tool!("apply_diff", "apply_diff", "Apply SEARCH/REPLACE blocks to modify files. Each block contains a SEARCH section and a REPLACE section.",
            ToolType::Stateless,
            obj_schema(vec![
                ("path".into(), prop("path", "string", true, "The absolute path to the file to modify")),
                ("diff".into(), prop("diff", "string", true, "The SEARCH/REPLACE diff content")),
            ], vec!["path".into(), "diff".into()]),
            metadata("filesystem", vec!["diff", "search-replace"])),

        tool!("list_files", "list_files", "List files and directories at the given path. Can be recursive.",
            ToolType::Stateless,
            obj_schema(vec![
                ("path".into(), prop("path", "string", true, "The absolute path to list")),
                ("recursive".into(), prop("recursive", "boolean", false, "Whether to list recursively")),
            ], vec!["path".into()]),
            metadata("filesystem", vec!["list", "file"])),

        tool!("grep_search", "grep_search", "Search file contents using a regular expression pattern. Returns matching file paths and line numbers.",
            ToolType::Stateless,
            obj_schema(vec![
                ("pattern".into(), prop("pattern", "string", true, "The regex pattern to search for")),
                ("path".into(), prop("path", "string", true, "The directory to search in")),
                ("include".into(), prop("include", "string", false, "File pattern to include (e.g. *.rs)")),
            ], vec!["pattern".into(), "path".into()]),
            metadata("filesystem", vec!["grep", "search"])),

        tool!("glob_search", "glob_search", "Find files matching a glob pattern. Returns matching file paths relative to the search path.",
            ToolType::Stateless,
            obj_schema(vec![
                ("pattern".into(), prop("pattern", "string", true, "The glob pattern to match")),
                ("path".into(), prop("path", "string", true, "The directory to search in")),
            ], vec!["pattern".into(), "path".into()]),
            metadata("filesystem", vec!["glob", "search"])),

        // ── Stateless: Shell ────────────────────────────
        tool!("execute_command", "execute_command", "Execute a shell command and capture its output. Supports configurable timeout and shell type.",
            ToolType::Stateless,
            obj_schema(vec![
                ("command".into(), prop("command", "string", true, "The shell command to execute")),
                ("timeout".into(), prop("timeout", "number", false, "Timeout in milliseconds")),
                ("cwd".into(), prop("cwd", "string", false, "Working directory for the command")),
            ], vec!["command".into()]),
            metadata("shell", vec!["shell", "command"])),

        // ── Stateless: Utility ──────────────────────────
        tool!("update_todo_list", "update_todo_list", "Update the current todo list with markdown-formatted tasks. Supports [ ], [-], [x] statuses.",
            ToolType::Stateless,
            obj_schema(vec![
                ("todos".into(), prop("todos", "string", true, "Markdown-formatted todo list with [ ], [-], [x] prefixes")),
            ], vec!["todos".into()]),
            metadata("utility", vec!["todo", "list"])),

        // ── Stateful: Memory ────────────────────────────
        tool!("record_note", "record_note", "Record a new session note for future recall. Notes are stored persistently and can be recalled later.",
            ToolType::Stateful,
            obj_schema(vec![
                ("note".into(), prop("note", "string", true, "The note content to record")),
                ("category".into(), prop("category", "string", false, "Optional category label for the note")),
            ], vec!["note".into()]),
            metadata("memory", vec!["note", "session"])),

        tool!("recall_notes", "recall_notes", "Recall previously recorded session notes. Filters by optional search term and category.",
            ToolType::Stateful,
            obj_schema(vec![
                ("search".into(), prop("search", "string", false, "Optional search term to filter notes")),
                ("category".into(), prop("category", "string", false, "Optional category to filter by")),
                ("limit".into(), prop("limit", "number", false, "Maximum number of notes to return")),
            ], vec![]),
            metadata("memory", vec!["note", "recall"])),

        tool!("list_categories", "list_categories", "List all note categories with note counts.",
            ToolType::Stateful,
            obj_schema(vec![], vec![]),
            metadata("memory", vec!["category"])),

        // ── Stateful: Shell ─────────────────────────────
        tool!("backend_shell", "backend_shell", "Start a long-running background shell session. Returns a session ID for subsequent operations.",
            ToolType::Stateful,
            obj_schema(vec![
                ("command".into(), prop("command", "string", true, "The command to start in the background")),
                ("cwd".into(), prop("cwd", "string", false, "Working directory")),
            ], vec!["command".into()]),
            metadata("shell", vec!["backend", "shell"])),

        tool!("shell_output", "shell_output", "Retrieve output from a running background shell session by session ID.",
            ToolType::Stateful,
            obj_schema(vec![
                ("session_id".into(), prop("session_id", "string", true, "The session ID from backend_shell")),
            ], vec!["session_id".into()]),
            metadata("shell", vec!["output"])),

        tool!("shell_kill", "shell_kill", "Kill a running background shell session by session ID.",
            ToolType::Stateful,
            obj_schema(vec![
                ("session_id".into(), prop("session_id", "string", true, "The session ID to kill")),
            ], vec!["session_id".into()]),
            metadata("shell", vec!["kill"])),

        // ── Builtin: Workflow ───────────────────────────
        tool!("execute_workflow", "execute_workflow", "Execute an existing workflow by ID. Optionally wait for completion.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("workflow_id".into(), prop("workflow_id", "string", true, "The ID of the workflow to execute")),
                ("input".into(), prop("input", "object", false, "Input parameters for the workflow")),
                ("wait".into(), prop("wait", "boolean", false, "Whether to wait for completion")),
            ], vec!["workflow_id".into()]),
            metadata("workflow", vec!["execute"])),

        tool!("query_workflow_status", "query_workflow_status", "Query the status of a running or completed workflow.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("workflow_id".into(), prop("workflow_id", "string", true, "The ID of the workflow")),
                ("execution_id".into(), prop("execution_id", "string", false, "Optional execution ID")),
            ], vec!["workflow_id".into()]),
            metadata("workflow", vec!["query", "status"])),

        tool!("cancel_workflow", "cancel_workflow", "Cancel a running workflow by ID.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("workflow_id".into(), prop("workflow_id", "string", true, "The ID of the workflow to cancel")),
            ], vec!["workflow_id".into()]),
            metadata("workflow", vec!["cancel"])),

        // ── Builtin: Agent ──────────────────────────────
        tool!("call_agent", "call_agent", "Call an agent by profile ID to execute a task. Optionally wait for completion.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("agent_profile_id".into(), prop("agent_profile_id", "string", true, "The agent profile ID")),
                ("prompt".into(), prop("prompt", "string", true, "The task prompt for the agent")),
                ("wait".into(), prop("wait", "boolean", false, "Whether to wait for completion")),
            ], vec!["agent_profile_id".into(), "prompt".into()]),
            metadata("agent", vec!["call"])),

        // ── Builtin: Interaction ────────────────────────
        tool!("ask_followup_question", "ask_followup_question", "Ask the user a follow-up question. Pauses execution until the user responds.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("question".into(), prop("question", "string", true, "The question to ask the user")),
                ("options".into(), prop("options", "array", false, "Optional multiple choice options")),
            ], vec!["question".into()]),
            metadata("interaction", vec!["ask"])),

        tool!("attempt_completion", "attempt_completion", "Signal that the task is complete. Provide a result summary and any state changes.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("result".into(), prop("result", "string", true, "Summary of what was accomplished")),
                ("variables".into(), prop("variables", "object", false, "State variable changes")),
            ], vec!["result".into()]),
            metadata("interaction", vec!["complete"])),

        // ── Builtin: Knowledge ──────────────────────────
        tool!("skill", "skill", "Load and apply a skill by name. Skills provide specialized instructions and workflows.",
            ToolType::BuiltIn,
            obj_schema(vec![
                ("skill".into(), prop("skill", "string", true, "The skill name to load")),
            ], vec!["skill".into()]),
            metadata("knowledge", vec!["skill"])),

        tool!("use_mcp", "use_mcp", "Call a tool or access a resource on an MCP server.",
            ToolType::Mcp,
            obj_schema(vec![
                ("server_name".into(), prop("server_name", "string", true, "The MCP server name")),
                ("tool_name".into(), prop("tool_name", "string", false, "The tool to call on the server")),
                ("arguments".into(), prop("arguments", "object", false, "Arguments for the tool")),
                ("uri".into(), prop("uri", "string", false, "The resource URI to read on the server")),
            ], vec!["server_name".into()]),
            metadata("integration", vec!["mcp"])),

        // ── Builtin: Web ────────────────────────────────
        tool!("web_search", "web_search", "Search the web for information. Returns relevant results with summaries.",
            ToolType::Stateless,
            obj_schema(vec![
                ("query".into(), prop("query", "string", true, "The search query")),
                ("max_results".into(), prop("max_results", "number", false, "Maximum number of results to return")),
            ], vec!["query".into()]),
            metadata("web", vec!["search"])),

        tool!("web_fetch", "web_fetch", "Fetch the content of a web page. Returns the page content in the requested format.",
            ToolType::Stateless,
            obj_schema(vec![
                ("url".into(), prop("url", "string", true, "The URL to fetch")),
                ("format".into(), prop("format", "string", false, "Output format: text, markdown, or html")),
            ], vec!["url".into()]),
            metadata("web", vec!["fetch"])),

        // ── Builtin: Memory (deprecated aliases) ────────
        tool!("memory_remember", "memory_remember", "Store information in long-term memory for future recall across sessions.",
            ToolType::Stateful,
            obj_schema(vec![
                ("key".into(), prop("key", "string", true, "The memory key")),
                ("content".into(), prop("content", "string", true, "The content to remember")),
            ], vec!["key".into(), "content".into()]),
            metadata("memory", vec!["remember"])),

        tool!("memory_forget", "memory_forget", "Remove a specific piece of information from long-term memory.",
            ToolType::Stateful,
            obj_schema(vec![
                ("key".into(), prop("key", "string", true, "The memory key to forget")),
            ], vec!["key".into()]),
            metadata("memory", vec!["forget"])),

        tool!("memory_list", "memory_list", "List all stored memories. Optionally filter by prefix.",
            ToolType::Stateful,
            obj_schema(vec![
                ("prefix".into(), prop("prefix", "string", false, "Optional prefix to filter by")),
            ], vec![]),
            metadata("memory", vec!["list"])),
    ]
}

pub fn register(regs: &Registries, opts: &Options) -> Summary {
    let mut total = Summary::new();
    for tool_def in builtin_tools() {
        let id = tool_def.id.clone();
        if crate::registrar::is_resource_disabled(&id, opts) {
            continue;
        }
        total.merge(register_item(
            &regs.tools,
            id,
            tool_def,
            opts.skip_if_exists,
        ));
    }
    total
}
