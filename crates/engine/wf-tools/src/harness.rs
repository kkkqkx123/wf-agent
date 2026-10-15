//! Fixture-driven example harness (feature `harness`).
//!
//! The harness drives every predefined tool through `ToolRegistry::execute_tool`
//! using JSON fixtures, then writes normalized results into the crate-local
//! `outputs/` directory so a real call's input and output can be reviewed and
//! diffed. It is intentionally smaller than the tool runtime: it only consumes
//! public APIs plus `serde_json`, `regex` and the standard library.
//!
//! Fixtures live under `fixtures/**/<case>.json`; one file is one case. A case
//! seeds an isolated temporary workspace, optionally runs a `setup` sequence of
//! tool calls (to exercise stateful stores), executes the observed tool call and
//! records the normalized result.

use std::fs;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    LoopFinishReason, SpawnedAgentLoop, SpawnedWorkflow, WorkflowInput, WorkflowOutput,
};
use crate::executor::trait_def::ToolExecutionContext;
use crate::filesystem::FsToolConfig;
use crate::general::GeneralToolInvoker;
use crate::handlers::{register_builtin_handlers, BuiltinHandlersConfig};
use crate::predefined;
use crate::registry::ToolRegistry;
use crate::skill::SkillLoader;
use wf_shell::config::ShellToolConfig;
use wf_types::skill::SkillConfig;
use wf_types::tool::{ToolExecutionOptions, ToolExecutionResult};

/// Default per-call timeout for harness executions.
pub const TIMEOUT_MS: u64 = 30_000;

/// Marker replacing the per-case sandbox path in normalized output.
const WORKSPACE_MARKER: &str = "<WORKSPACE>";
/// Marker replacing generated unique ids in normalized output.
const ID_MARKER: &str = "<ID>";
/// Marker replacing generated timestamps in normalized output.
const TIMESTAMP_MARKER: &str = "<TIMESTAMP>";
/// Sub-directory holding results of tiers that are not committed as golden
/// output (process / network / mcp), keeping them out of the diff.
const VOLATILE_DIR: &str = "_volatile";
/// Placeholder a `network`-tier fixture embeds in a string parameter; the
/// runner replaces it with the in-process mock server's base URL.
pub const MOCK_HTTP_BASE: &str = "<MOCK_HTTP_BASE>";

/// External dependency tier of a tool, deciding whether it runs by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Offline,
    Process,
    Stub,
    Network,
    Mcp,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Offline => "offline",
            Tier::Process => "process",
            Tier::Stub => "stub",
            Tier::Network => "network",
            Tier::Mcp => "mcp",
        }
    }

    pub fn parse(s: &str) -> Option<Tier> {
        match s {
            "offline" => Some(Tier::Offline),
            "process" => Some(Tier::Process),
            "stub" => Some(Tier::Stub),
            "network" => Some(Tier::Network),
            "mcp" => Some(Tier::Mcp),
            _ => None,
        }
    }

    /// Tiers executed by a default run (no network, no subprocesses).
    pub fn default_enabled(self) -> bool {
        matches!(self, Tier::Offline | Tier::Stub)
    }

    /// Tiers whose results are environment-dependent and therefore not
    /// committed as golden output.
    pub fn is_volatile(self) -> bool {
        !self.default_enabled()
    }
}

/// The tier assigned to a tool id, derived from its category. `read_file_folded`
/// degrades to a plain local read when the code-context service is absent.
///
/// Category alone is not enough: the `general` builtin is filed under the
/// `utility` category but resolves through an injected engine invoker, so a few
/// builtin ids are pinned explicitly before the category fallback.
pub fn tool_tier(id: &str) -> Tier {
    match id {
        "read_file_folded" => return Tier::Offline,
        "execute_workflow"
        | "query_workflow_status"
        | "cancel_workflow"
        | "call_agent"
        | "skill"
        | "general"
        | "ask_followup_question"
        | "attempt_completion" => return Tier::Stub,
        "use_mcp" => return Tier::Mcp,
        _ => {}
    }
    match predefined::all_definitions()
        .into_iter()
        .find(|d| d.id == id)
        .map(|d| d.category)
    {
        Some("filesystem") | Some("memory") | Some("utility") => Tier::Offline,
        Some("code_context") | Some("web") => Tier::Network,
        Some("shell") => Tier::Process,
        Some("integration") => Tier::Mcp,
        _ => Tier::Stub,
    }
}

fn default_parameters() -> Value {
    json!({})
}

/// One fixture file: a tool invocation plus its sandbox seed and assertions.
#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub tool: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub workspace: Option<WorkspaceSeed>,
    /// Tool calls executed before the observed call, sharing the same registry.
    #[serde(default)]
    pub setup: Vec<Step>,
    #[serde(default = "default_parameters")]
    pub parameters: Value,
    #[serde(default)]
    pub context: Option<ContextSeed>,
    #[serde(default)]
    pub expect: Option<Expect>,
}

/// Files materialized into the per-case temporary workspace.
#[derive(Debug, Deserialize, Default)]
pub struct WorkspaceSeed {
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
}

/// A preparatory tool call, executed for its side effects only.
#[derive(Debug, Deserialize)]
pub struct Step {
    pub tool: String,
    #[serde(default = "default_parameters")]
    pub parameters: Value,
}

/// Execution context overrides.
#[derive(Debug, Deserialize, Default)]
pub struct ContextSeed {
    #[serde(default)]
    pub execution_id: Option<String>,
}

/// Optional assertions over the raw result.
#[derive(Debug, Deserialize, Default)]
pub struct Expect {
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub error_contains: Option<String>,
    #[serde(default)]
    pub result_contains: Option<String>,
}

/// A loaded fixture case (file path + parsed content).
pub struct Case {
    pub path: PathBuf,
    pub name: String,
    pub fixture: Fixture,
}

/// Outcome of running a single case.
pub struct CaseOutcome {
    pub tool: String,
    pub name: String,
    pub tier: Tier,
    /// Normalized record written to `outputs/<tool>/<name>.json`.
    pub record: Value,
    pub failures: Vec<String>,
    /// Set when the tool call itself returned a `ToolError`.
    pub execution_error: Option<String>,
}

impl CaseOutcome {
    /// A case passes when no assertion failed and the tool call was dispatched
    /// without a registry-level error.
    pub fn passed(&self) -> bool {
        self.failures.is_empty() && self.execution_error.is_none()
    }
}

/// Drives fixtures and manages the crate-local fixtures/outputs directories.
pub struct Runner {
    pub fixtures_dir: PathBuf,
    pub outputs_dir: PathBuf,
    temp_root: PathBuf,
}

impl Runner {
    pub fn new(fixtures_dir: PathBuf, outputs_dir: PathBuf) -> Self {
        let temp_root =
            std::env::temp_dir().join(format!("wf-tools-harness-{}", std::process::id()));
        Self {
            fixtures_dir,
            outputs_dir,
            temp_root,
        }
    }

    /// Load every fixture file under `fixtures_dir` (recursively).
    pub fn load_cases(&self) -> io::Result<Vec<Case>> {
        let mut files = Vec::new();
        collect_json_files(&self.fixtures_dir, &mut files)?;
        files.sort();
        let mut cases = Vec::new();
        for path in files {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('_') {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let fixture: Fixture = serde_json::from_str(&text).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid fixture {}: {}", path.display(), e),
                )
            })?;
            cases.push(Case {
                path,
                name,
                fixture,
            });
        }
        Ok(cases)
    }

    /// Run one case in an isolated sandbox.
    pub async fn run_case(&self, case: &Case) -> CaseOutcome {
        let fixture = &case.fixture;
        let tier = fixture
            .tier
            .as_deref()
            .and_then(Tier::parse)
            .unwrap_or_else(|| tool_tier(&fixture.tool));

        let sandbox = self
            .temp_root
            .join(format!("{}-{}", fixture.tool, case.name));
        let _ = fs::remove_dir_all(&sandbox);
        if let Err(e) = fs::create_dir_all(&sandbox) {
            return CaseOutcome {
                tool: fixture.tool.clone(),
                name: case.name.clone(),
                tier,
                record: Value::Null,
                failures: Vec::new(),
                execution_error: Some(format!("sandbox creation failed: {e}")),
            };
        }
        seed_workspace(&sandbox, fixture.workspace.as_ref());

        let registry = match build_registry(&sandbox).await {
            Ok(registry) => registry,
            Err(e) => {
                return CaseOutcome {
                    tool: fixture.tool.clone(),
                    name: case.name.clone(),
                    tier,
                    record: Value::Null,
                    failures: Vec::new(),
                    execution_error: Some(e.to_string()),
                };
            }
        };

        let execution_id = fixture
            .context
            .as_ref()
            .and_then(|c| c.execution_id.clone())
            .unwrap_or_else(|| "fixture-exec".to_string());

        // Setup steps run for their side effects; their results are discarded.
        let base = mock_http_base();
        for step in &fixture.setup {
            let params = substitute_mock_base(&step.parameters, base);
            let _ = call_tool(&registry, &step.tool, &params, &execution_id).await;
        }

        let params = substitute_mock_base(&fixture.parameters, base);
        let raw = call_tool(&registry, &fixture.tool, &params, &execution_id).await;

        let record = self.record(case, tier, &raw, &sandbox);
        let failures = evaluate(&fixture.expect, &raw);
        let execution_error = raw.as_ref().err().map(|e| e.to_string());

        CaseOutcome {
            tool: fixture.tool.clone(),
            name: case.name.clone(),
            tier,
            record,
            failures,
            execution_error,
        }
    }

    fn record(
        &self,
        case: &Case,
        tier: Tier,
        raw: &Result<ToolExecutionResult, crate::error::ToolError>,
        sandbox: &Path,
    ) -> Value {
        let fixture = &case.fixture;
        let (sandbox_str, canonical) = sandbox_markers(sandbox);
        let mut base = json!({
            "tool": fixture.tool,
            "case": case.name,
            "tier": tier.as_str(),
            "description": fixture.description,
            "parameters": fixture.parameters,
        });
        let result = match raw {
            Ok(res) => {
                let mut value = serde_json::to_value(res).unwrap_or(Value::Null);
                normalize(&mut value, &sandbox_str, &canonical);
                value
            }
            Err(e) => {
                let mut message = e.to_string();
                message = normalize_string(&message, &sandbox_str, &canonical);
                json!({ "tool_error": message })
            }
        };
        base["result"] = result;
        base
    }

    /// Write the per-tool schema, doc examples and every case record.
    pub fn write_outputs(&self, outcomes: &[CaseOutcome], write_meta_all: bool) -> io::Result<()> {
        fs::create_dir_all(&self.outputs_dir)?;

        if write_meta_all {
            for def in predefined::all_definitions() {
                self.write_tool_meta(def.id, &def.tool_def())?;
            }
        } else {
            let mut tools: Vec<&str> = outcomes.iter().map(|o| o.tool.as_str()).collect();
            tools.sort();
            tools.dedup();
            for id in tools {
                if let Some(def) = predefined::all_definitions()
                    .into_iter()
                    .find(|d| d.id == id)
                {
                    self.write_tool_meta(def.id, &def.tool_def())?;
                }
            }
        }

        for outcome in outcomes {
            // Volatile tiers (process / network / mcp) are environment
            // dependent, so their records are isolated under `_volatile` and
            // never committed as golden output.
            let base = if outcome.tier.is_volatile() {
                self.outputs_dir.join(VOLATILE_DIR)
            } else {
                self.outputs_dir.clone()
            };
            let dir = base.join(&outcome.tool);
            fs::create_dir_all(&dir)?;
            write_json(&dir.join(format!("{}.json", outcome.name)), &outcome.record)?;
        }

        // The index summarizes the full tool set, so it is only rewritten by
        // a full run; a filtered run keeps the committed index intact.
        if write_meta_all {
            self.write_index(outcomes)?;
        }
        Ok(())
    }

    fn write_tool_meta(&self, id: &str, tool: &wf_types::tool::Tool) -> io::Result<()> {
        let dir = self.outputs_dir.join(id);
        fs::create_dir_all(&dir)?;
        let schema = serde_json::to_value(tool).unwrap_or(Value::Null);
        write_json(&dir.join("schema.json"), &schema)?;

        let def = predefined::all_definitions()
            .into_iter()
            .find(|d| d.id == id);
        let mut text = String::new();
        if let Some(def) = def {
            text.push_str(&format!("# {} ({})\n", def.id, def.category));
            text.push_str(def.description);
            text.push('\n');
            if let Some(tips) = def.tips {
                text.push_str("\ntips:\n");
                for tip in tips {
                    text.push_str(&format!("- {}\n", tip));
                }
            }
            if let Some(examples) = def.examples {
                text.push_str("\nexamples:\n");
                for example in examples {
                    text.push_str(&format!("- {}\n", example));
                }
            }
        }
        fs::write(dir.join("doc_examples.txt"), text)?;
        Ok(())
    }

    fn write_index(&self, outcomes: &[CaseOutcome]) -> io::Result<()> {
        let mut tools = std::collections::BTreeMap::new();
        for outcome in outcomes {
            let entry = tools
                .entry(outcome.tool.clone())
                .or_insert_with(|| json!({ "tier": outcome.tier.as_str(), "cases": [] }));
            if let Some(cases) = entry["cases"].as_array_mut() {
                cases.push(json!({
                    "case": outcome.name,
                    "passed": outcome.passed(),
                    "execution_error": outcome.execution_error,
                }));
            }
        }
        let index = json!({
            "tools": tools,
            "cases": outcomes.len(),
        });
        write_json(&self.outputs_dir.join("_index.json"), &index)
    }

    /// Compare freshly generated records against the committed outputs.
    /// Returns human-readable drift descriptions (empty means in sync).
    /// Volatile tiers are skipped: they are not committed as golden output.
    pub fn check(&self, outcomes: &[CaseOutcome]) -> io::Result<Vec<String>> {
        let mut drift = Vec::new();
        for outcome in outcomes {
            if outcome.tier.is_volatile() {
                continue;
            }
            let path = self
                .outputs_dir
                .join(&outcome.tool)
                .join(format!("{}.json", outcome.name));
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let committed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                    if committed != outcome.record {
                        drift.push(format!(
                            "{}: output drift (run with --update to refresh)",
                            path.display()
                        ));
                    }
                }
                Err(_) => drift.push(format!("{}: missing golden output", path.display())),
            }
        }
        Ok(drift)
    }
}

/// Deterministic execution callback for builtin dispatch tools.
///
/// `call_agent` / `execute_workflow` / `query_workflow_status` /
/// `cancel_workflow` resolve through an injected engine callback; the harness
/// substitutes canned responses so the dispatch contract (parameter parsing,
/// wait/async branching, result shaping) is exercised without a real engine.
/// Every value is fixed or derived from the request, so outputs stay stable.
struct StubCallback;

#[async_trait]
impl ExecutionCallback for StubCallback {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> crate::error::ToolResult<AgentLoopOutput> {
        Ok(AgentLoopOutput {
            agent_loop_id: "stub-agent-loop".to_string(),
            result: json!({
                "stub": "agent_loop",
                "agent_id": config.agent_id,
                "model": config.model,
                "prompt": input.message,
            }),
            iterations: 1,
            finish_reason: LoopFinishReason::Completed,
            conversation: Vec::new(),
        })
    }

    async fn spawn_agent_loop(
        &self,
        _config: AgentLoopConfig,
        _input: AgentLoopInput,
    ) -> crate::error::ToolResult<SpawnedAgentLoop> {
        Ok(SpawnedAgentLoop {
            agent_loop_id: "stub-agent-loop".to_string(),
            execution_id: "stub-agent-exec".to_string(),
            status: "started".to_string(),
        })
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> crate::error::ToolResult<WorkflowOutput> {
        Ok(WorkflowOutput {
            execution_id: "stub-workflow-exec".to_string(),
            result: json!({
                "stub": "workflow",
                "workflow_id": workflow_id,
                "variables": input.variables,
            }),
        })
    }

    async fn spawn_workflow(
        &self,
        _workflow_id: &str,
        _input: WorkflowInput,
    ) -> crate::error::ToolResult<SpawnedWorkflow> {
        Ok(SpawnedWorkflow {
            execution_id: "stub-workflow-exec".to_string(),
            status: "started".to_string(),
        })
    }

    async fn query_execution_status(
        &self,
        execution_id: &str,
        _caller_root: &wf_types::Id,
    ) -> crate::error::ToolResult<ExecutionStatus> {
        Ok(ExecutionStatus {
            execution_id: execution_id.to_string(),
            status: "completed".to_string(),
            progress: Some(1.0),
            result: Some(json!({ "stub": "status" })),
        })
    }

    async fn cancel_execution(
        &self,
        _execution_id: &str,
        _caller_root: &wf_types::Id,
    ) -> crate::error::ToolResult<()> {
        Ok(())
    }
}

/// Deterministic inner-tool resolver for the `general` proxy tool.
///
/// The real invoker routes requests back through the engine pipeline; the
/// harness only needs to confirm the request body is forwarded intact and
/// that a malformed body yields the self-correct text, so it echoes the
/// parsed request and mirrors the format-error contract on parse failure.
struct StubGeneralInvoker;

#[async_trait]
impl GeneralToolInvoker for StubGeneralInvoker {
    async fn invoke_request(&self, request: &str) -> crate::error::ToolResult<Value> {
        match serde_json::from_str::<Value>(request) {
            Ok(value) => Ok(json!({ "stub": "general", "request": value })),
            Err(_) => Ok(Value::String(
                "Invalid general request. Expected a JSON object \
                 {\"tool\": \"tool_name\", \"parameters\": {...}}."
                    .to_string(),
            )),
        }
    }
}

/// Base URL of the in-process mock HTTP server, started lazily on first use
/// and shared for the process lifetime. Fixtures reach it by embedding
/// [`MOCK_HTTP_BASE`] in a string parameter (the runner substitutes the real
/// origin before executing).
pub fn mock_http_base() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock http server");
        let addr = listener
            .local_addr()
            .expect("resolve mock http server address");
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle_mock_http(stream);
            }
        });
        format!("http://{addr}")
    })
}

/// Canned DuckDuckGo-style HTML the mock server returns (one result link plus
/// snippet) so `web_search`'s parser produces stable output.
const MOCK_SEARCH_HTML: &str = "<html><body>\n<a class=\"result__a\" href=\"//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Frust-async\">Rust Async Programming</a>\n<div class=\"result__snippet\">Learn async Rust from the book.</div>\n</body></html>\n";

/// Canned HTML page returned for `/page`, used by `web_fetch`.
const MOCK_PAGE_HTML: &str =
    "<!DOCTYPE html>\n<html><body><h1>Mock Page</h1><p>Mock page body.</p></body></html>";

fn handle_mock_http(mut stream: TcpStream) {
    // Resolve the request path, then drop the reader before responding.
    // Only headers are read (up to the blank line); reading to EOF would
    // deadlock because the client keeps its write half open awaiting the
    // response.
    let path = {
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line == "\r\n" || line == "\n" => break,
                Ok(_) => {}
            }
        }
        request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_string()
    };

    let body = if path.starts_with("/page") {
        MOCK_PAGE_HTML
    } else {
        MOCK_SEARCH_HTML
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Replace the [`MOCK_HTTP_BASE`] placeholder in every string of a JSON value.
fn substitute_mock_base(value: &Value, base: &str) -> Value {
    match value {
        Value::String(s) if s.contains(MOCK_HTTP_BASE) => {
            Value::String(s.replace(MOCK_HTTP_BASE, base))
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| substitute_mock_base(v, base))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), substitute_mock_base(v, base)))
                .collect(),
        ),
        other => other.clone(),
    }
}

async fn build_registry(sandbox: &Path) -> crate::error::ToolResult<ToolRegistry> {
    let registry = ToolRegistry::new();
    let config = BuiltinHandlersConfig {
        fs: FsToolConfig {
            workspace_dir: Some(sandbox.to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        },
        shell: ShellToolConfig {
            workspace_dir: Some(sandbox.to_path_buf()),
            ..Default::default()
        },
        web: predefined::web::WebToolConfig {
            search_endpoint: Some(format!("{}/search", mock_http_base())),
            ..Default::default()
        },
        ..Default::default()
    };
    register_builtin_handlers(&registry, config)?;

    // Builtin dispatch tools resolve through the injected callback; scanning
    // the per-case `skills/` directory backs the `skill` tool. Both are read
    // when an executor is built, so setting them before the first call is
    // enough.
    registry.set_builtin_callback(Arc::new(StubCallback));
    let skills_dir = sandbox.join("skills");
    registry.set_skill_loader(Arc::new(SkillLoader::new(SkillConfig {
        paths: vec![skills_dir.to_string_lossy().to_string()],
        auto_scan: Some(true),
    })));

    // Keep the session-note database inside the sandbox instead of the process
    // working directory: re-registering the factory overrides the default.
    let db_path = sandbox.join("data/session-notes.db");
    if let Some(db) = db_path.to_str() {
        let _ = predefined::memory::session_note::register(&registry, Some(db));
    }

    for def in predefined::all_definitions() {
        registry.register_tool(def.tool_def());
    }
    Ok(registry)
}

async fn call_tool(
    registry: &ToolRegistry,
    tool_id: &str,
    parameters: &Value,
    execution_id: &str,
) -> crate::error::ToolResult<ToolExecutionResult> {
    let ctx = ToolExecutionContext::new(execution_id.to_string())
        .with_general_invoker(Arc::new(StubGeneralInvoker));
    let options = ToolExecutionOptions {
        timeout: Some(TIMEOUT_MS),
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };
    registry
        .execute_tool(tool_id, parameters, &options, &ctx)
        .await
}

fn seed_workspace(sandbox: &Path, seed: Option<&WorkspaceSeed>) {
    let Some(seed) = seed else {
        return;
    };
    for (rel, content) in &seed.files {
        let path = sandbox.join(rel);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, content);
    }
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out)?;
        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}

fn evaluate(
    expect: &Option<Expect>,
    raw: &Result<ToolExecutionResult, crate::error::ToolError>,
) -> Vec<String> {
    let Some(expect) = expect else {
        return Vec::new();
    };
    let mut failures = Vec::new();
    match raw {
        Ok(res) => {
            if let Some(want) = expect.success {
                if res.success != want {
                    failures.push(format!("expected success={want}, got {}", res.success));
                }
            }
            if let Some(sub) = &expect.error_contains {
                let err = res.error.clone().unwrap_or_default();
                if !err.contains(sub) {
                    failures.push(format!("error {:?} does not contain {:?}", err, sub));
                }
            }
            if let Some(sub) = &expect.result_contains {
                let text = res
                    .result
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                if !text.contains(sub) {
                    failures.push(format!("result does not contain {:?}", sub));
                }
            }
        }
        Err(e) => {
            let message = e.to_string();
            if expect.success == Some(true) {
                failures.push(format!("expected success but the call failed: {message}"));
            }
            if let Some(sub) = &expect.error_contains {
                if !message.contains(sub) {
                    failures.push(format!(
                        "tool error {:?} does not contain {:?}",
                        message, sub
                    ));
                }
            }
        }
    }
    failures
}

fn sandbox_markers(sandbox: &Path) -> (String, String) {
    let shown = sandbox.to_string_lossy().to_string();
    let canonical = sandbox
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| shown.clone());
    (shown, canonical)
}

fn normalize(value: &mut Value, sandbox: &str, canonical: &str) {
    match value {
        Value::String(s) => {
            *s = normalize_string(s, sandbox, canonical);
        }
        Value::Array(items) => {
            for item in items {
                normalize(item, sandbox, canonical);
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    "execution_time" | "created_at" | "updated_at" | "duration_ms" => {
                        *child = json!(0);
                    }
                    "timestamp" => {
                        *child = if child.is_string() {
                            json!(TIMESTAMP_MARKER)
                        } else {
                            json!(0)
                        };
                    }
                    _ => normalize(child, sandbox, canonical),
                }
            }
        }
        _ => {}
    }
}

fn normalize_string(s: &str, sandbox: &str, canonical: &str) -> String {
    let mut out = s.replace(sandbox, WORKSPACE_MARKER);
    if canonical != sandbox {
        out = out.replace(canonical, WORKSPACE_MARKER);
    }
    uuid_regex().replace_all(&out, ID_MARKER).to_string()
}

fn uuid_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .expect("static uuid regex compiles")
    })
}

fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    text.push('\n');
    fs::write(path, text)
}
