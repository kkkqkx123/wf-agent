//! Remote deployment: install, start and remove a `wf-server` binary on a
//! registered host over the same system `ssh` transport the tunnel uses.
//!
//! Placement follows the Zed / VS Code model: everything lands under the
//! login user's home, no root, no package manager, no system service. The
//! layout is
//!
//! ```text
//! ~/.wf-server/
//!   bin/<version>/wf-server   immutable once written
//!   current -> bin/<version>  symlink, swapped on upgrade
//!   server/                   transferred settings layer (`{root}/server/*.toml`)
//!   scripts/                  transferred helper scripts
//!   run/wf-server.pid         pid of the started process
//!   run/launch.sh             generated start shim
//!   logs/wf-server.log        stdout/stderr of the started process
//!   install.json              what was installed, from where, when
//! ```
//!
//! A per-version directory plus one symlink makes upgrade a link swap and
//! rollback the previous target; the same property is what lets a running
//! process keep its own binary while a new one is written beside it.
//!
//! Three rules shape every remote command here:
//!
//! * The command shape is fixed. Caller input reaches the remote shell only
//!   through quoted arguments, validated relative paths and stdin payloads.
//!   There is no free-form shell parameter, so the write surface stays
//!   enumerable without a verb whitelist.
//! * Every installed file is written under a temporary name and renamed, so
//!   an interrupted transfer never leaves a half-written binary that the
//!   `current` link could resolve to.
//! * The artifact is fetched by the remote host by default. Uploading a
//!   local binary is an explicit opt-in, because the manager and the target
//!   frequently run different architectures.
//!
//! The module keeps no desired state and runs no reconcile loop. `probe` is
//! the only verification surface: it reports the remote facts plus a verdict,
//! and every other verb is a single request whose result is observed once.

use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::types::{is_safe_name_component, BackendEntry};

/// Install root under the remote login home.
pub const DEFAULT_INSTALL_ROOT: &str = ".wf-server";
/// Default base URL for release artifacts. The tail follows the same module
/// naming contract `deploy/scripts/fetch-package.sh` uses, so one published
/// directory feeds both the single-host installer and remote deployment.
pub const DEFAULT_ARTIFACT_BASE_URL: &str = "https://packages.example.com/wf-agent";
/// Default local directory holding deploy sources (`server/`, `scripts/`).
pub const DEFAULT_SOURCE_ROOT: &str = "deploy";
/// Audit value used when the transport cannot distinguish API-key callers.
pub const DEFAULT_SUBMITTED_BY: &str = "api-key";
/// Largest single transferred configuration or script file.
const MAX_FILE_BYTES: usize = 1024 * 1024;
/// Largest combined payload of one deployment request.
const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// Lines returned by a backend log read when the caller does not choose.
pub const DEFAULT_REMOTE_LOG_LINES: usize = 200;
/// Largest line count a single log read may request.
pub const MAX_REMOTE_LOG_LINES: usize = 2000;
/// Largest log text one response carries; the newest bytes win.
const MAX_REMOTE_LOG_BYTES: usize = 64 * 1024;
/// Upper bound on any single remote command.
const DEFAULT_EXEC_TIMEOUT_SECS: u64 = 120;
/// How long a start waits for the process to answer its health endpoint.
const DEFAULT_START_TIMEOUT_SECS: u64 = 30;
/// How long a stop waits for a term signal before escalating.
const STOP_GRACE_POLLS: u32 = 20;

/// Deployment knobs resolved from `backends.toml`. They only describe where,
/// from where and how long, never what: the verb set is fixed by this module.
#[derive(Debug, Clone)]
pub struct DeploySettings {
    /// Directory under the remote home. Rejected when it escapes the home.
    pub install_root: String,
    /// Base URL the remote host downloads release artifacts from.
    pub artifact_base_url: String,
    /// Local directory holding deploy sources. Relative paths resolve
    /// against the management server's working directory.
    pub source_root: std::path::PathBuf,
    pub exec_timeout_secs: u64,
    pub start_timeout_secs: u64,
}

impl Default for DeploySettings {
    fn default() -> Self {
        Self {
            install_root: DEFAULT_INSTALL_ROOT.to_string(),
            artifact_base_url: DEFAULT_ARTIFACT_BASE_URL.to_string(),
            source_root: std::path::PathBuf::from(DEFAULT_SOURCE_ROOT),
            exec_timeout_secs: DEFAULT_EXEC_TIMEOUT_SECS,
            start_timeout_secs: DEFAULT_START_TIMEOUT_SECS,
        }
    }
}

/// Deployment verb. Each one maps to a fixed command sequence; none of them
/// accepts additional shell text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeployAction {
    /// Fetch the remote facts and report, changing nothing.
    Probe,
    /// Write the artifact plus payload files, without starting anything.
    Install,
    /// Start the installed binary and wait for it to answer.
    Start,
    /// Stop the started process.
    Stop,
    /// Stop, install, start.
    Upgrade,
    /// Stop and remove the install root, including configuration and logs.
    Uninstall,
}

impl DeployAction {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim() {
            "probe" => Ok(Self::Probe),
            "install" => Ok(Self::Install),
            "start" => Ok(Self::Start),
            "stop" => Ok(Self::Stop),
            "upgrade" => Ok(Self::Upgrade),
            "uninstall" => Ok(Self::Uninstall),
            other => Err(format!(
                "unknown deploy action '{other}'; expected probe, install, start, stop, upgrade or uninstall"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Install => "install",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Upgrade => "upgrade",
            Self::Uninstall => "uninstall",
        }
    }

    /// Whether the action writes to the remote host.
    pub fn writes(self) -> bool {
        !matches!(self, Self::Probe)
    }
}

/// Where the binary comes from.
///
/// `release` is the default: the remote host downloads the published
/// `wf-agent-server-<version>-<os>-<arch>.tar.gz` for its own platform, so a
/// manager running on a different architecture can still deploy. `url`
/// points at one explicit archive or bare binary. `remote_path` reuses a
/// binary the host already has. `local` uploads a local file — or this
/// management server's own executable — and is the only source that needs
/// the two machines to share an architecture.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ArtifactSpec {
    /// `release`, `url`, `remote_path` or `local`.
    #[serde(default)]
    pub kind: String,
    /// URL, remote path or local path, depending on `kind`. Ignored by
    /// `release`, which derives the URL from the version and platform.
    #[serde(default)]
    pub path: String,
    /// Version to install. Absent means the management server's version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional expected SHA-256 for the artifact bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// How much of the deploy source travels with the request.
///
/// Only two coarse scopes exist on purpose: anything finer is a targeted
/// change that belongs on a direct `ssh`, not in a deployment API that would
/// then need per-file policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployScope {
    /// Deploy no files; the remote keeps whatever it already has.
    None,
    /// `server/*.toml` only — the settings layer of the remote server.
    Config,
    /// `server/*.toml` plus `scripts/*` — configuration and helper scripts.
    Full,
}

impl DeployScope {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim() {
            "" | "none" => Ok(Self::None),
            "config" => Ok(Self::Config),
            "full" => Ok(Self::Full),
            other => Err(format!(
                "unknown deploy scope '{other}'; expected none, config or full"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Config => "config",
            Self::Full => "full",
        }
    }

    /// Whether the scope transfers scripts as well as configuration.
    pub fn includes_scripts(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// One deployment request. Serialized form is also the HTTP body and the
/// CLI payload, so the three frontends cannot drift.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployRequest {
    /// Verb as a string so an unknown value fails with a readable message
    /// instead of a serde enum error.
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default)]
    pub artifact: Option<ArtifactSpec>,
    /// `none`, `config` or `full`. Absent means `none`.
    #[serde(default)]
    pub scope: String,
    /// Deploy source directory, relative to the configured source root.
    /// Absent means the source root itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Allow an action whose preconditions reported `blocked`.
    #[serde(default)]
    pub force: bool,
    /// Digest of the approved plan. Required for every writing action and
    /// ignored by `probe`; `force` never substitutes for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

fn default_action() -> String {
    "probe".to_string()
}

fn default_submitted_by() -> String {
    DEFAULT_SUBMITTED_BY.to_string()
}

impl DeployRequest {
    pub fn parsed_action(&self) -> Result<DeployAction, String> {
        DeployAction::parse(&self.action)
    }

    pub fn parsed_scope(&self) -> Result<DeployScope, String> {
        DeployScope::parse(&self.scope)
    }

    /// Whether this request needs an approval digest before it may run.
    pub fn requires_plan_id(&self) -> bool {
        self.parsed_action()
            .map(|action| action.writes())
            .unwrap_or(true)
    }
}

/// Facts read from the remote host in one round trip.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployTarget {
    /// `uname -s`, e.g. `Linux`.
    pub os: String,
    /// `uname -m`, e.g. `x86_64`.
    pub arch: String,
    /// `<os>-<arch>` lowercased, the label used in reports.
    pub platform: String,
    pub home: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Whether `curl` exists remotely; health waits degrade without it.
    pub curl: bool,
    /// `wf-server` found on `PATH`, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wf_server_on_path: Option<String>,
}

/// What is already installed under the install root.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RemoteInstallation {
    pub root: String,
    /// Resolved `current` link target, e.g. `bin/0.1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Version reported by the installed binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Versions present under `bin/`, newest name last.
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// The recorded pid is alive.
    pub running: bool,
    /// The process answers its loopback health endpoint.
    pub healthy: bool,
    /// Directories present under the root: `bin`, `current`, `server`,
    /// `scripts`, `run`, `logs`.
    #[serde(default)]
    pub dirs: Vec<String>,
    #[serde(default)]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub script_files: Vec<String>,
    /// Contents of `install.json` when it parses, verbatim otherwise absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_from: Option<String>,
}

/// One verification item reported by `probe`. Deployment has no reconcile
/// loop, so this list is the whole "did it work" answer: a failed item names
/// what is missing instead of leaving the operator to read raw fields.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployCheckView {
    /// Stable id, e.g. `installed`, `running`, `healthy`, `version_match`.
    pub id: String,
    pub ok: bool,
    /// Short human-readable explanation of the pass or the failure.
    pub detail: String,
}

/// Probe result: what the host is, what is installed, and what failed.
/// Unreachable hosts are data, matching the status views: one dead backend
/// must not fail the list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployProbeView {
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<DeployTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<RemoteInstallation>,
    /// `ok`, `degraded`, `absent` or `unreachable`.
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub checks: Vec<DeployCheckView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub probed_ms: i64,
}

/// One deploy source directory the management server can read.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeploySourceView {
    /// Directory name relative to the configured source root; empty for the
    /// root itself.
    pub name: String,
    /// Absolute path on the management server, for the operator to edit.
    pub path: String,
    #[serde(default)]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub script_files: Vec<String>,
    /// Why this directory cannot serve `config`/`full`, when it cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// One planned remote action. The plan is what the operator approves, and
/// it is the literal command that runs, so approval cannot cover something
/// else.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployStepView {
    pub id: String,
    pub description: String,
    /// Remote command as it will be executed, without stdin payloads.
    pub command: String,
}

/// A transfer announced in the plan: path, bytes and mode.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlannedFileView {
    pub path: String,
    pub bytes: usize,
    pub mode: String,
}

/// Resolved artifact for one deployment.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ArtifactPlanView {
    pub kind: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// `ok`, `missing_local_file`, `platform_mismatch` or `unknown`.
    pub status: String,
}

/// Dry run: everything the action would do, plus what blocks it today.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployPlanView {
    /// Digest of the execution-relevant plan content. Stable across
    /// timestamp-only changes so it can be approved before execution.
    pub plan_id: String,
    pub backend: String,
    pub action: String,
    pub install_root: String,
    pub server_port: u16,
    /// `none`, `config` or `full`.
    #[serde(default)]
    pub scope: String,
    /// Selected source directory relative to the configured source root.
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<DeployTarget>,
    /// Current installation, so a `probe` plan is a complete status report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<RemoteInstallation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactPlanView>,
    #[serde(default)]
    pub files: Vec<PlannedFileView>,
    #[serde(default)]
    pub steps: Vec<DeployStepView>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Present when the action cannot run as planned; `force` does not
    /// bypass a missing artifact, only a version or platform disagreement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    pub planned_ms: i64,
}

/// Execution-relevant part of a plan. Timestamps and the current install
/// snapshot are deliberately absent: neither changes what the steps do, and
/// including them would invalidate every approval a moment later.
#[derive(Serialize)]
struct PlanFingerprint<'a> {
    backend: &'a str,
    action: &'a str,
    install_root: &'a str,
    server_port: u16,
    scope: &'a str,
    source: &'a str,
    target_platform: Option<&'a str>,
    artifact: Option<&'a ArtifactPlanView>,
    files: &'a [PlannedFileView],
    steps: &'a [DeployStepView],
    blocked: Option<&'a str>,
}

/// Stable digest of one plan. The same request, remote facts and deploy
/// source always produce the same id; any command, file or target change
/// produces a different one.
pub fn plan_fingerprint(plan: &DeployPlanView) -> String {
    let relevant = PlanFingerprint {
        backend: &plan.backend,
        action: &plan.action,
        install_root: &plan.install_root,
        server_port: plan.server_port,
        scope: &plan.scope,
        source: &plan.source,
        target_platform: plan.target.as_ref().map(|target| target.platform.as_str()),
        artifact: plan.artifact.as_ref(),
        files: &plan.files,
        steps: &plan.steps,
        blocked: plan.blocked.as_deref(),
    };
    let bytes = serde_json::to_vec(&relevant).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn with_plan_id(mut plan: DeployPlanView) -> DeployPlanView {
    plan.plan_id = plan_fingerprint(&plan);
    plan
}

/// Check that an approval still describes the plan about to be executed.
/// A mismatch is an authorization failure, not a condition `force` may
/// override.
pub(crate) fn require_approved_plan(
    action: DeployAction,
    approved: Option<&str>,
    actual: &str,
) -> Result<(), String> {
    if !action.writes() {
        return Ok(());
    }
    let Some(approved) = approved.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("plan_id is required for write actions; run deploy plan first".to_string());
    };
    if approved != actual {
        return Err(format!(
            "plan changed since approval ({approved} != {actual}); run deploy plan again before applying"
        ));
    }
    Ok(())
}

/// Per-step execution result. A failing step keeps the rest unrun, so the
/// index of the failure is the whole diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployStepResultView {
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Trimmed stderr, bounded so one noisy step cannot flood the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
}

/// Result of one applied action, followed by a fresh probe so the caller
/// never has to chain a second request to learn the new state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployOutcomeView {
    pub backend: String,
    pub action: String,
    pub ok: bool,
    pub install_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub steps: Vec<DeployStepResultView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<DeployProbeView>,
    pub finished_ms: i64,
    /// Owning job, when this outcome was produced by the background job path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// `succeeded`, `failed` or `interrupted`. Missing means `succeeded`
    /// for outcomes written before the job path existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Status of one background deployment job.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeployJobView {
    pub job_id: String,
    pub backend: String,
    pub action: String,
    /// `queued`, `running`, `succeeded`, `failed` or `interrupted`.
    pub status: String,
    /// Number of steps in the plan that the background task settled on.
    pub total_steps: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    /// Cancellation was requested for this job.
    pub cancel_requested: bool,
    /// Audit subject. The transport has no per-key identity, so every
    /// submission is recorded as `api-key` rather than a guessed user.
    #[serde(default = "default_submitted_by")]
    pub submitted_by: String,
    /// Plan recomputed by apply, including the steps actually executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_plan: Option<DeployPlanView>,
    #[serde(default)]
    pub steps: Vec<DeployStepResultView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<DeployOutcomeView>,
    pub submitted_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<i64>,
}

/// One step transition reported by `apply` to the job owner.
#[derive(Debug, Clone)]
pub struct DeployStepProgress {
    pub step_id: String,
    /// `running` before the command starts, `done` after it settles.
    pub state: &'static str,
    pub total_steps: usize,
    pub result: Option<DeployStepResultView>,
}

impl DeployStepProgress {
    fn running(step_id: &str, total_steps: usize) -> Self {
        Self {
            step_id: step_id.to_string(),
            state: "running",
            total_steps,
            result: None,
        }
    }

    fn done(result: DeployStepResultView, total_steps: usize) -> Self {
        Self {
            step_id: result.id.clone(),
            state: "done",
            total_steps,
            result: Some(result),
        }
    }
}

/// One finished remote command.
#[derive(Debug, Clone)]
pub(crate) struct ExecOutput {
    status: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

impl ExecOutput {
    fn success(&self) -> bool {
        !self.timed_out && self.status == Some(0)
    }

    fn trimmed_stderr(&self) -> Option<String> {
        trim_bounded(&self.stderr)
    }

    fn trimmed_stdout(&self) -> Option<String> {
        trim_bounded(&self.stdout)
    }
}

/// Cap on how much remote output one step reports back.
const MAX_REPORTED_OUTPUT_BYTES: usize = 4096;

fn trim_bounded(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    if out.len() > MAX_REPORTED_OUTPUT_BYTES {
        let mut cut = MAX_REPORTED_OUTPUT_BYTES;
        while cut > 0 && !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push_str("...");
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// One SSH command channel for a backend entry.
///
/// Argument shape mirrors `TunnelSupervisor::tunnel_argv` so a live tunnel's
/// ControlMaster is reused when present: deployment then costs one channel
/// on an existing connection instead of a fresh handshake per step.
pub(crate) struct DeployChannel<'a> {
    entry: &'a BackendEntry,
    local_port: u16,
    timeout: Duration,
}

impl<'a> DeployChannel<'a> {
    pub(crate) fn new(entry: &'a BackendEntry, local_port: u16, timeout: Duration) -> Self {
        Self {
            entry,
            local_port,
            timeout,
        }
    }

    /// Build the `ssh` argv for one fixed remote command.
    pub(crate) fn ssh_argv(&self, command: &str) -> Vec<String> {
        let target = if self.entry.username.trim().is_empty() {
            self.entry.host.clone()
        } else {
            format!("{}@{}", self.entry.username, self.entry.host)
        };
        let mut argv = vec![
            "ssh".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ConnectTimeout=10".to_string(),
            "-p".to_string(),
            self.entry.port.unwrap_or(22).to_string(),
            "-o".to_string(),
            "ControlMaster=no".to_string(),
            "-o".to_string(),
            format!(
                "ControlPath={}",
                wf_types::script::schema::tunnel_control_socket(&self.entry.host, self.local_port)
                    .display()
            ),
        ];
        if let Some(key) = self
            .entry
            .key_path
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            argv.push("-i".to_string());
            argv.push(key.to_string());
        }
        if let Some(jump) = self
            .entry
            .jump_host
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            argv.push("-J".to_string());
            argv.push(jump.to_string());
        }
        argv.push(target);
        argv.push(command.to_string());
        argv
    }

    /// Run one remote command, optionally feeding stdin.
    ///
    /// `kill_on_drop` plus one timeout around the payload write and the
    /// command wait bounds every call, including a host that never reads
    /// the uploaded binary.
    pub(crate) async fn run(
        &self,
        command: &str,
        stdin: Option<Vec<u8>>,
    ) -> Result<ExecOutput, String> {
        use tokio::io::AsyncWriteExt;

        let argv = self.ssh_argv(command);
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| "empty ssh argv".to_string())?;
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdin(if stdin.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot start ssh for backend '{}': {e}", self.entry.host))?;
        let command = async {
            if let Some(bytes) = stdin {
                if let Some(mut pipe) = child.stdin.take() {
                    pipe.write_all(&bytes)
                        .await
                        .map_err(|e| format!("cannot write deployment payload: {e}"))?;
                    pipe.shutdown()
                        .await
                        .map_err(|e| format!("cannot close deployment payload: {e}"))?;
                }
            }
            child
                .wait_with_output()
                .await
                .map_err(|e| format!("ssh command failed: {e}"))
        };
        Ok(run_bounded(self.timeout, async {
            let output = command.await?;
            Ok(ExecOutput {
                status: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                timed_out: false,
            })
        })
        .await)
    }
}

/// One timeout boundary for an entire remote operation, including stdin
/// writes. Dropping the future on timeout also drops the ssh child held by
/// the caller, whose `kill_on_drop` guard stops the process.
pub(crate) async fn run_bounded<F>(timeout: Duration, operation: F) -> ExecOutput
where
    F: Future<Output = Result<ExecOutput, String>>,
{
    match tokio::time::timeout(timeout, operation).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => ExecOutput {
            status: None,
            stdout: String::new(),
            stderr: e,
            timed_out: false,
        },
        Err(_) => ExecOutput {
            status: None,
            stdout: String::new(),
            stderr: format!(
                "remote command exceeded {}s and was aborted",
                timeout.as_secs()
            ),
            timed_out: true,
        },
    }
}

/// Read the newest lines of the deployed server log.
///
/// The command shape is fixed: a bounded line count and a path derived from
/// the normalized install root. No caller string reaches the remote shell.
pub(crate) async fn read_log(
    name: &str,
    entry: &BackendEntry,
    settings: &DeploySettings,
    local_port: u16,
    lines: usize,
) -> Result<(String, bool), String> {
    let lines = lines.clamp(1, MAX_REMOTE_LOG_LINES);
    let command = format!("tail -n {lines} {}", log_path(&settings.install_root));
    let channel = DeployChannel::new(
        entry,
        local_port,
        Duration::from_secs(settings.exec_timeout_secs.max(1)),
    );
    let output = channel.run(&command, None).await?;
    if !output.success() {
        let detail = trim_bounded(&output.stderr)
            .unwrap_or_else(|| "no diagnostic output".to_string());
        return Err(format!(
            "cannot read remote log for backend '{name}': {detail}"
        ));
    }
    Ok(bound_log_tail(&output.stdout))
}

/// Keep the newest bytes of a log response so a few very long lines cannot
/// push the useful tail out of one bounded reply.
fn bound_log_tail(raw: &str) -> (String, bool) {
    if raw.len() <= MAX_REMOTE_LOG_BYTES {
        return (raw.to_string(), false);
    }
    let mut start = raw.len() - MAX_REMOTE_LOG_BYTES;
    while start < raw.len() && !raw.is_char_boundary(start) {
        start += 1;
    }
    (format!("...{}", &raw[start..]), true)
}

// ---------------------------------------------------------------------------
// Paths and quoting
// ---------------------------------------------------------------------------

/// Quote one shell word with POSIX single quotes. Kept local rather than
/// shared with the client crate: that one is private and this module must
/// stay usable from the server alone.
fn shell_quote(raw: &str) -> String {
    let mut quoted = String::with_capacity(raw.len() + 2);
    quoted.push('\'');
    for chunk in raw.split('\'') {
        quoted.push_str(chunk);
        quoted.push_str("'\\''");
    }
    quoted.truncate(quoted.len() - 4);
    quoted.push('\'');
    quoted
}

/// Validate one install-root setting. A relative name is joined to the home
/// directory; nothing may climb out of it, because the deployment then owns
/// files outside its stated root.
fn normalize_install_root(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let name = trimmed.strip_prefix("./").unwrap_or(trimmed);
    if name.is_empty() {
        return Err("deploy install_root must not be empty".to_string());
    }
    if name.starts_with('~') || name.starts_with('/') {
        return Err(format!(
            "deploy install_root '{raw}' must be relative to the remote home directory"
        ));
    }
    for component in name.split('/') {
        if !is_safe_name_component(component) {
            return Err(format!(
                "deploy install_root '{raw}' must use only letters, digits, '_', '-', '.' and single '/' separators"
            ));
        }
    }
    Ok(name.to_string())
}

/// Mode assigned by destination directory. Configuration is owner-read
/// only because it can carry tokens; scripts must be executable to run.
fn mode_for(path: &str) -> &'static str {
    if path.starts_with("scripts/") {
        "700"
    } else {
        "600"
    }
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

/// Probe script. One round trip, tab-separated records, no `set -e` so a
/// missing optional tool does not end the report early.
///
/// Every emitted key is fixed here; the parser never evaluates remote data.
fn probe_script(root: &str, health_port: Option<u16>) -> String {
    let quoted_root = shell_quote(root);
    let health = health_port
        .map(|port| {
            format!(
                "if [ -f \"$root/run/wf-server.pid\" ]; then pid=$(cat \"$root/run/wf-server.pid\" 2>/dev/null); fi\n\
                 if [ -n \"${{pid:-}}\" ] && kill -0 \"$pid\" 2>/dev/null; then printf 'pid\\t%s\\n' \"$pid\"; fi\n\
                 if command -v curl >/dev/null 2>&1; then if curl -fsS --max-time 2 'http://127.0.0.1:{port}/health' >/dev/null 2>&1; then printf 'healthy\\t1\\n'; fi; fi\n"
            )
        })
        .unwrap_or_default();
    format!(
        "printf 'os\\t%s\\n' \"$(uname -s)\"\n\
         printf 'arch\\t%s\\n' \"$(uname -m)\"\n\
         printf 'home\\t%s\\n' \"$HOME\"\n\
         printf 'shell\\t%s\\n' \"${{SHELL:-}}\"\n\
         if command -v curl >/dev/null 2>&1; then printf 'curl\\t1\\n'; else printf 'curl\\t0\\n'; fi\n\
         if command -v wf-server >/dev/null 2>&1; then printf 'path\\t%s\\n' \"$(command -v wf-server)\"; fi\n\
         root=\"$HOME\"/{quoted_root}\n\
         if [ -d \"$root\" ]; then\n\
           printf 'root\\t%s\\n' \"$root\"\n\
           for d in bin current server scripts run logs; do if [ -d \"$root/$d\" ]; then printf 'dir\\t%s\\n' \"$d\"; fi; done\n\
           if [ -L \"$root/current\" ]; then printf 'current\\t%s\\n' \"$(readlink \"$root/current\")\"; fi\n\
           if [ -x \"$root/current/wf-server\" ]; then printf 'version\\t%s\\n' \"$(\"$root/current/wf-server\" --version 2>/dev/null | head -n1)\"; fi\n\
           for d in \"$root\"/bin/*; do if [ -d \"$d\" ]; then printf 'bin\\t%s\\n' \"$(basename \"$d\")\"; fi; done\n\
           for f in \"$root\"/server/*.toml; do if [ -f \"$f\" ]; then printf 'conf\\t%s\\n' \"$(basename \"$f\")\"; fi; done\n\
           for f in \"$root\"/scripts/*; do if [ -f \"$f\" ]; then printf 'script\\t%s\\n' \"$(basename \"$f\")\"; fi; done\n\
           {health}\
         fi\n"
    )
}

/// Parsed probe records.
#[derive(Debug, Default)]
struct ProbeRecord {
    target: DeployTarget,
    installation: Option<RemoteInstallation>,
}

fn parse_probe(stdout: &str, root: &str) -> ProbeRecord {
    let mut target = DeployTarget::default();
    let mut installation = RemoteInstallation {
        root: root.to_string(),
        ..RemoteInstallation::default()
    };
    let mut saw_root = false;
    let mut versions = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, '\t');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default().trim();
        match key {
            "os" => target.os = value.to_string(),
            "arch" => target.arch = value.to_string(),
            "home" => target.home = value.to_string(),
            "shell" => target.shell = Some(value.to_string()).filter(|s| !s.is_empty()),
            "curl" => target.curl = value == "1",
            "path" => target.wf_server_on_path = Some(value.to_string()).filter(|s| !s.is_empty()),
            "root" => {
                saw_root = true;
                installation.root = value.to_string();
            }
            "current" => installation.current = Some(value.to_string()),
            "version" => installation.version = Some(clean_version(value)),
            "bin" => versions.push(value.to_string()),
            "dir" => installation.dirs.push(value.to_string()),
            "conf" => installation.config_files.push(value.to_string()),
            "script" => installation.script_files.push(value.to_string()),
            "pid" => {
                installation.pid = value.parse().ok();
                installation.running = installation.pid.is_some();
            }
            "healthy" => installation.healthy = value == "1",
            _ => {}
        }
    }
    target.platform = format!(
        "{}-{}",
        target.os.to_lowercase(),
        normalize_arch(&target.arch)
    );
    versions.sort();
    versions.dedup();
    installation.versions = versions;
    ProbeRecord {
        target,
        installation: saw_root.then_some(installation),
    }
}

/// `wf-server 0.1.0` and bare `0.1.0` both reduce to `0.1.0`.
fn clean_version(raw: &str) -> String {
    raw.trim()
        .rsplit(' ')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Normalize `uname -m` into the target triple architecture name used by
/// local platform reports, so a mismatch check compares like with like.
fn normalize_arch(raw: &str) -> String {
    match raw.trim() {
        "amd64" => "x86_64".to_string(),
        "arm64" => "aarch64".to_string(),
        other => other.to_lowercase(),
    }
}

/// Local platform label, from the compiled target.
fn local_platform() -> String {
    format!(
        "{}-{}",
        std::env::consts::OS,
        normalize_arch(std::env::consts::ARCH)
    )
}

/// Turn one probe record into the "did the deployment work" answer.
///
/// Four checks decide the verdict: `installed`, `layout`, `running` and
/// `health`. Configuration, script count and version are informational —
/// a `config`-only deployment legitimately has no scripts, and an operator
/// may intentionally install a version other than the manager's.
fn evaluate_probe(
    installation: Option<&RemoteInstallation>,
    target: &DeployTarget,
) -> (String, Vec<DeployCheckView>) {
    let mut checks: Vec<DeployCheckView> = Vec::new();
    let mut add = |id: &str, ok: bool, detail: String| {
        checks.push(DeployCheckView {
            id: id.to_string(),
            ok,
            detail,
        });
    };
    let Some(install) = installation else {
        add(
            "installed",
            false,
            "no install directory under the remote home".to_string(),
        );
        return ("absent".to_string(), checks);
    };
    let installed = install.current.is_some() && install.version.is_some();
    add(
        "installed",
        installed,
        match install.version.as_deref() {
            Some(version) => format!(
                "current -> {}, version {version}",
                install.current.as_deref().unwrap_or("(unset)")
            ),
            None => "current link or --version is missing".to_string(),
        },
    );
    let required = ["bin", "current", "server", "scripts"];
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|dir| !install.dirs.iter().any(|present| present == dir))
        .collect();
    add(
        "layout",
        missing.is_empty(),
        if missing.is_empty() {
            "bin, current, server and scripts are present".to_string()
        } else {
            format!("missing directories: {}", missing.join(", "))
        },
    );
    add(
        "running",
        install.running,
        match install.pid {
            Some(pid) => format!("pid {pid} is alive"),
            None => "no live pid recorded in run/wf-server.pid".to_string(),
        },
    );
    // Without curl the remote cannot answer an HTTP health check; process
    // liveness is then the strongest signal available and is reported as
    // such rather than silently passing as "healthy".
    let health_ok = if target.curl {
        install.healthy
    } else {
        install.running
    };
    add(
        "health",
        health_ok,
        if !target.curl {
            "curl unavailable on the remote host; liveness only".to_string()
        } else if install.healthy {
            "loopback /health answered".to_string()
        } else {
            "loopback /health did not answer".to_string()
        },
    );
    add(
        "config",
        !install.config_files.is_empty(),
        format!("{} server/*.toml file(s)", install.config_files.len()),
    );
    add(
        "scripts",
        !install.script_files.is_empty(),
        format!("{} script file(s)", install.script_files.len()),
    );
    add(
        "version",
        install.version.is_some(),
        install
            .version
            .clone()
            .unwrap_or_else(|| "version unknown".to_string()),
    );
    let critical = ["installed", "layout", "running", "health"];
    let ok = checks
        .iter()
        .filter(|check| critical.contains(&check.id.as_str()))
        .all(|check| check.ok);
    (if ok { "ok" } else { "degraded" }.to_string(), checks)
}

/// Probe one backend. Never fails: transport errors land in `error` so the
/// caller renders a row instead of a 500. This is also the single
/// verification surface: no desired state is stored and nothing reconciles.
pub async fn probe(
    backend: &str,
    entry: &BackendEntry,
    settings: &DeploySettings,
    local_port: u16,
) -> DeployProbeView {
    let started = wf_common::epoch_ms();
    let root = match normalize_install_root(&settings.install_root) {
        Ok(root) => root,
        Err(e) => {
            return DeployProbeView {
                backend: backend.to_string(),
                error: Some(e),
                verdict: "unreachable".to_string(),
                probed_ms: started,
                ..DeployProbeView::default()
            };
        }
    };
    let channel = DeployChannel::new(
        entry,
        local_port,
        Duration::from_secs(settings.exec_timeout_secs.max(1)),
    );
    let script = probe_script(&root, Some(entry.effective_server_port()));
    match channel.run(&script, None).await {
        Ok(output) if output.success() => {
            let record = parse_probe(&output.stdout, &root);
            let (verdict, checks) = evaluate_probe(record.installation.as_ref(), &record.target);
            DeployProbeView {
                backend: backend.to_string(),
                target: Some(record.target),
                installation: record.installation,
                verdict,
                checks,
                error: None,
                probed_ms: started,
            }
        }
        Ok(output) => DeployProbeView {
            backend: backend.to_string(),
            error: Some(
                output
                    .trimmed_stderr()
                    .or_else(|| output.trimmed_stdout())
                    .unwrap_or_else(|| "remote probe returned no output".to_string()),
            ),
            verdict: "unreachable".to_string(),
            probed_ms: started,
            ..DeployProbeView::default()
        },
        Err(e) => DeployProbeView {
            backend: backend.to_string(),
            error: Some(e),
            verdict: "unreachable".to_string(),
            probed_ms: started,
            ..DeployProbeView::default()
        },
    }
}

// ---------------------------------------------------------------------------
// Artifact resolution
// ---------------------------------------------------------------------------

/// Resolved artifact bytes plus provenance. `local` carries the bytes to
/// upload; the other kinds carry only a remote-side locator. `archive`
/// marks a tar.gz that must be unpacked before the binary can run.
#[derive(Debug, Clone)]
enum ArtifactPayload {
    Local {
        bytes: Vec<u8>,
    },
    Remote {
        path: String,
    },
    Url {
        url: String,
        sha256: Option<String>,
        archive: bool,
    },
}

#[derive(Debug, Clone)]
struct ResolvedArtifact {
    payload: ArtifactPayload,
    view: ArtifactPlanView,
    /// Non-empty when the artifact cannot be used as-is.
    blk: Option<String>,
}

/// A download source is unusable on a host without `curl`: the transfer
/// step would fail mid-deployment, after the layout had already changed.
/// Report it in the plan instead, so the operator can switch to a local or
/// remote-path artifact before anything runs.
fn download_transport_problem(payload: &ArtifactPayload, target: &DeployTarget) -> Option<String> {
    if target.curl || !matches!(payload, ArtifactPayload::Url { .. }) {
        return None;
    }
    Some(
        "remote host has no curl; the default artifact download needs it. Install curl or pass --artifact-kind local|remote_path"
            .to_string(),
    )
}

/// Build the release URL from the same naming contract the single-host
/// package fetcher uses: `<base>/wf-agent-server-<version>-<platform>.tar.gz`.
fn release_artifact_url(base: &str, version: &str, platform: &str) -> String {
    format!(
        "{}/wf-agent-server-{}-{}.tar.gz",
        base.trim_end_matches('/'),
        version.trim(),
        platform.trim()
    )
}

/// Resolve the artifact for one deployment.
///
/// The default is a remote download. It names the archive for the target's
/// own platform, so a macOS manager can deploy to a Linux ARM host; the
/// manager's version is the requested version unless the caller overrides
/// it. Uploading a local binary stays available as an explicit opt-in for
/// air-gapped hosts and for developing an unversioned build.
fn resolve_artifact(
    spec: &Option<ArtifactSpec>,
    target: &DeployTarget,
    settings: &DeploySettings,
) -> Result<ResolvedArtifact, String> {
    let spec = spec.clone().unwrap_or_default();
    let kind = if spec.kind.trim().is_empty() {
        "release".to_string()
    } else {
        spec.kind.trim().to_string()
    };
    let local = local_platform();
    match kind.as_str() {
        "release" => {
            let base = settings.artifact_base_url.trim();
            if base.is_empty() {
                return Ok(ResolvedArtifact {
                    payload: ArtifactPayload::Url {
                        url: String::new(),
                        sha256: spec.sha256.clone(),
                        archive: true,
                    },
                    view: ArtifactPlanView {
                        kind,
                        source: String::new(),
                        bytes: None,
                        sha256: spec.sha256.clone(),
                        status: "missing_base_url".to_string(),
                    },
                    blk: Some(
                        "deploy_artifact_base_url is empty; configure it or pass --artifact-kind local|url|remote_path"
                            .to_string(),
                    ),
                });
            }
            let version = spec
                .version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(env!("CARGO_PKG_VERSION"));
            let url = release_artifact_url(base, version, &target.platform);
            Ok(ResolvedArtifact {
                payload: ArtifactPayload::Url {
                    url: url.clone(),
                    sha256: spec.sha256.clone(),
                    archive: true,
                },
                view: ArtifactPlanView {
                    kind,
                    source: url,
                    bytes: None,
                    sha256: spec.sha256.clone(),
                    status: "ok".to_string(),
                },
                blk: None,
            })
        }
        "local" | "local_path" => {
            let path = if spec.path.trim().is_empty() {
                std::env::current_exe()
                    .map_err(|e| format!("cannot locate the management server executable: {e}"))?
                    .display()
                    .to_string()
            } else {
                spec.path.trim().to_string()
            };
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Ok(ResolvedArtifact {
                        payload: ArtifactPayload::Local { bytes: Vec::new() },
                        view: ArtifactPlanView {
                            kind,
                            source: path.clone(),
                            bytes: None,
                            sha256: None,
                            status: "missing_local_file".to_string(),
                        },
                        blk: Some(format!("cannot read local artifact {path}: {e}")),
                    });
                }
            };
            let sha256 = hex::encode(Sha256::digest(&bytes));
            let mut warnings_block = None;
            if let Some(expected) = spec.sha256.as_deref().filter(|s| !s.trim().is_empty()) {
                if !expected.trim().eq_ignore_ascii_case(&sha256) {
                    warnings_block = Some(format!(
                        "local artifact sha256 {sha256} does not match the requested {}",
                        expected.trim()
                    ));
                }
            }
            if target.platform != local {
                warnings_block = Some(format!(
                    "local artifact is built for {local} but the remote host is {}; use --artifact-kind release or provide a matching local binary",
                    target.platform
                ));
            }
            Ok(ResolvedArtifact {
                payload: ArtifactPayload::Local {
                    bytes: bytes.clone(),
                },
                view: ArtifactPlanView {
                    kind,
                    source: path,
                    bytes: Some(bytes.len() as u64),
                    sha256: Some(sha256),
                    status: if warnings_block.is_some() {
                        "platform_mismatch".to_string()
                    } else {
                        "ok".to_string()
                    },
                },
                blk: warnings_block,
            })
        }
        "remote_path" => {
            let path = spec.path.trim().to_string();
            if path.is_empty() {
                return Err("artifact kind 'remote_path' requires a path".to_string());
            }
            Ok(ResolvedArtifact {
                payload: ArtifactPayload::Remote { path: path.clone() },
                view: ArtifactPlanView {
                    kind,
                    source: path,
                    bytes: None,
                    sha256: spec.sha256.clone(),
                    status: "ok".to_string(),
                },
                blk: None,
            })
        }
        "url" => {
            let url = spec.path.trim().to_string();
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return Err("artifact kind 'url' requires an http(s) URL".to_string());
            }
            let archive = url.ends_with(".tar.gz") || url.ends_with(".tgz");
            Ok(ResolvedArtifact {
                payload: ArtifactPayload::Url {
                    url: url.clone(),
                    sha256: spec.sha256.clone(),
                    archive,
                },
                view: ArtifactPlanView {
                    kind,
                    source: url,
                    bytes: None,
                    sha256: spec.sha256.clone(),
                    status: "ok".to_string(),
                },
                blk: None,
            })
        }
        other => Err(format!(
            "unknown artifact kind '{other}'; expected release, local, remote_path or url"
        )),
    }
}

// ---------------------------------------------------------------------------
// Deploy sources
// ---------------------------------------------------------------------------

/// One validated transfer: destination, bytes, and the mode to apply.
type PayloadFile = (String, Vec<u8>, &'static str);

/// The configured source root must exist on the management server. It is a
/// local directory: the API caller only ever names a child of it.
fn source_root(settings: &DeploySettings) -> Result<std::path::PathBuf, String> {
    let raw = settings.source_root.as_path();
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot resolve the working directory: {e}"))?
            .join(raw)
    };
    let canonical = candidate.canonicalize().map_err(|e| {
        format!(
            "deploy source root '{}' is not available: {e}",
            candidate.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "deploy source root '{}' is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

/// Resolve one source directory inside the configured root. Absolute paths,
/// `..` and symlinks leaving the root are rejected: a deployment request
/// must not turn into an arbitrary file read on the management server.
fn resolve_source_dir(
    settings: &DeploySettings,
    name: Option<&str>,
) -> Result<(std::path::PathBuf, String), String> {
    let root = source_root(settings)?;
    let raw = name.unwrap_or("").trim();
    if raw.is_empty() {
        return Ok((root, String::new()));
    }
    let cleaned = raw.strip_prefix("./").unwrap_or(raw);
    if std::path::Path::new(cleaned).is_absolute() || cleaned.starts_with('~') {
        return Err(format!(
            "deploy source '{raw}' must be a directory name relative to the source root"
        ));
    }
    for component in cleaned.split('/') {
        if !is_safe_name_component(component) {
            return Err(format!(
                "deploy source '{raw}' has an illegal component '{component}'"
            ));
        }
    }
    let canonical = root.join(cleaned).canonicalize().map_err(|e| {
        format!(
            "deploy source '{raw}' not found under '{}': {e}",
            root.display()
        )
    })?;
    if !canonical.starts_with(&root) || !canonical.is_dir() {
        return Err(format!(
            "deploy source '{raw}' must stay inside '{}' and be a directory",
            root.display()
        ));
    }
    Ok((canonical, cleaned.to_string()))
}

/// List the source directories an operator may pass to `--from` / the web
/// panel, together with what each one would transfer.
pub fn list_sources(settings: &DeploySettings) -> Vec<DeploySourceView> {
    let root = match source_root(settings) {
        Ok(root) => root,
        Err(_) => return Vec::new(),
    };
    let mut dirs: Vec<(String, std::path::PathBuf)> = vec![(String::new(), root.clone())];
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if let Ok(kind) = entry.file_type() {
                if kind.is_dir() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !name.starts_with('.') && is_safe_name_component(&name) {
                        dirs.push((name, entry.path()));
                    }
                }
            }
        }
    }
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    dirs.into_iter()
        .map(|(name, path)| {
            let config_files = list_source_names(&path.join("server"), "toml");
            let script_files = list_source_names(&path.join("scripts"), "");
            let problem = if config_files.is_empty() {
                Some("no server/*.toml files".to_string())
            } else if script_files.is_empty() {
                Some("no scripts/* files; only scope=config is available".to_string())
            } else {
                None
            };
            DeploySourceView {
                name,
                path: path.display().to_string(),
                config_files,
                script_files,
                problem,
            }
        })
        .collect()
}

fn list_source_names(dir: &std::path::Path, extension: &str) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !extension.is_empty()
                && path.extension().and_then(|value| value.to_str()) != Some(extension)
            {
                continue;
            }
            if is_safe_name_component(name) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// Read the files selected by `scope` from one source directory.
///
/// Only two shapes exist: `server/*.toml` (config) and `scripts/*` (full).
/// An empty selection is an error for config/full, so a typo in `--from`
/// cannot silently deploy a server with no settings.
fn read_source_files(
    settings: &DeploySettings,
    source: Option<&str>,
    scope: DeployScope,
) -> Result<Vec<PayloadFile>, String> {
    if scope == DeployScope::None {
        return Ok(Vec::new());
    }
    let (dir, label) = resolve_source_dir(settings, source)?;
    let mut selections: Vec<(String, std::path::PathBuf)> = Vec::new();
    for name in list_source_names(&dir.join("server"), "toml") {
        selections.push((format!("server/{name}"), dir.join("server").join(&name)));
    }
    if scope.includes_scripts() {
        for name in list_source_names(&dir.join("scripts"), "") {
            selections.push((format!("scripts/{name}"), dir.join("scripts").join(&name)));
        }
    }
    if selections.is_empty() {
        return Err(format!(
            "deploy source '{}' has no files for scope {}",
            if label.is_empty() { "." } else { &label },
            scope.as_str()
        ));
    }
    let mut out: Vec<PayloadFile> = Vec::new();
    let mut total = 0usize;
    for (destination, path) in selections {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("cannot read deploy source '{}': {e}", path.display()))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!(
                "deployed file '{destination}' is {} bytes; the per-file limit is {MAX_FILE_BYTES}",
                bytes.len()
            ));
        }
        // Configuration and scripts are text; a binary slipped into the
        // source tree is not something this API should transfer.
        if std::str::from_utf8(&bytes).is_err() {
            return Err(format!("deployed file '{destination}' is not valid UTF-8"));
        }
        total += bytes.len();
        if total > MAX_TOTAL_BYTES {
            return Err(format!(
                "deployed payload exceeds {MAX_TOTAL_BYTES} bytes in total"
            ));
        }
        let mode = if destination.starts_with("scripts/") {
            "700"
        } else {
            "600"
        };
        out.push((destination, bytes, mode));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Command sequences
// ---------------------------------------------------------------------------

/// Remote paths derived from the install root, quoted once here so every
/// command agrees on the same layout.
struct Layout {
    root: String,
}

impl Layout {
    fn new(root: &str) -> Self {
        Self {
            root: format!("\"$HOME\"/{}", shell_quote(root)),
        }
    }
}

/// Fixed path of the process log under one install root.
pub(crate) fn log_path(root: &str) -> String {
    format!("{}/logs/wf-server.log", Layout::new(root).root)
}

fn mkdir_step(layout: &Layout) -> DeployStepView {
    DeployStepView {
        id: "layout".to_string(),
        description: "Create the versioned install layout under the remote home".to_string(),
        command: format!(
            "mkdir -p {}/bin {}/server {}/scripts {}/run {}/logs",
            layout.root, layout.root, layout.root, layout.root, layout.root
        ),
    }
}

fn install_steps(layout: &Layout, artifact: &ArtifactPayload) -> Vec<DeployStepView> {
    let mut steps = vec![mkdir_step(layout)];
    match artifact {
        ArtifactPayload::Local { .. } => {
            steps.push(DeployStepView {
                id: "upload".to_string(),
                description: "Stream the artifact to a temporary name and make it executable"
                    .to_string(),
                command: format!(
                    "cat > {}/bin/upload.tmp && chmod 700 {}/bin/upload.tmp && {}/bin/upload.tmp --version",
                    layout.root, layout.root, layout.root
                ),
            });
        }
        ArtifactPayload::Remote { path } => {
            steps.push(DeployStepView {
                id: "upload".to_string(),
                description: "Copy the existing remote binary into the versioned layout"
                    .to_string(),
                command: format!(
                    "cp {} {}/bin/upload.tmp && chmod 700 {}/bin/upload.tmp && {}/bin/upload.tmp --version",
                    shell_quote(path),
                    layout.root,
                    layout.root,
                    layout.root
                ),
            });
        }
        ArtifactPayload::Url {
            url,
            sha256,
            archive,
        } => {
            // The published package layout is `wf-server` + `web/`, the same
            // one `fetch-package.sh` produces. A bare binary skips unpacking.
            // A checksum is enforced when supplied; a sibling `.sha256` is
            // used when published and merely omitted when absent.
            let (verify, prepare) = if *archive {
                let checksum = match sha256.as_deref().filter(|value| !value.trim().is_empty()) {
                    Some(expected) => format!(
                        "printf '%s  %s\\n' {} {b}/bin/download.tmp | sha256sum -c -",
                        shell_quote(expected.trim()),
                        b = layout.root
                    ),
                    None => format!(
                        "if curl -fsSL {} -o {b}/bin/download.tmp.sha256 2>/dev/null; then printf '%s  %s\\n' \"$(awk '{{print $1}}' {b}/bin/download.tmp.sha256)\" {b}/bin/download.tmp | sha256sum -c -; else echo 'no published checksum; continuing'; fi",
                        shell_quote(&format!("{url}.sha256")),
                        b = layout.root
                    ),
                };
                (
                    format!(" && {checksum}"),
                    format!(
                        " && mkdir -p {b}/bin/unpack.tmp && tar -xzf {b}/bin/download.tmp -C {b}/bin/unpack.tmp && cp {b}/bin/unpack.tmp/wf-server {b}/bin/upload.tmp && rm -rf {b}/bin/unpack.tmp {b}/bin/download.tmp",
                        b = layout.root
                    ),
                )
            } else {
                let verify = match sha256.as_deref().filter(|value| !value.trim().is_empty()) {
                    Some(expected) => format!(
                        " && printf '%s  %s\\n' {} {b}/bin/download.tmp | sha256sum -c -",
                        shell_quote(expected.trim()),
                        b = layout.root
                    ),
                    None => String::new(),
                };
                (
                    verify,
                    format!(
                        " && mv {b}/bin/download.tmp {b}/bin/upload.tmp",
                        b = layout.root
                    ),
                )
            };
            steps.push(DeployStepView {
                id: "upload".to_string(),
                description: "Download the artifact on the remote host and verify it runs"
                    .to_string(),
                command: format!(
                    "curl -fsSL {} -o {b}/bin/download.tmp{verify}{prepare} && chmod 700 {b}/bin/upload.tmp && {b}/bin/upload.tmp --version",
                    shell_quote(url),
                    b = layout.root
                ),
            });
        }
    }
    steps.push(DeployStepView {
        id: "activate".to_string(),
        description:
            "Move the verified binary into its version directory and swap the current link"
                .to_string(),
        command: format!(
            "set -e; v=$({b}/bin/upload.tmp --version | head -n1 | awk '{{print $NF}}'); \
             mkdir -p {b}/bin/$v; mv {b}/bin/upload.tmp {b}/bin/$v/wf-server; \
             ln -sfn bin/$v {b}/current; printf '%s\\n' \"$v\"",
            b = layout.root
        ),
    });
    steps.push(DeployStepView {
        id: "record".to_string(),
        description: "Record what was installed so the next probe can report it".to_string(),
        command: format!(
            "cat > {}/install.json.tmp && mv {}/install.json.tmp {}/install.json",
            layout.root, layout.root, layout.root
        ),
    });
    steps
}

fn file_step(layout: &Layout, path: &str) -> DeployStepView {
    let mode = mode_for(path);
    DeployStepView {
        id: format!("file:{path}"),
        description: format!("Write {} with mode {mode}", path),
        command: format!(
            "cat > {}/.upload.tmp && chmod {mode} {}/.upload.tmp && mv {}/.upload.tmp {}/{}",
            layout.root, layout.root, layout.root, layout.root, path
        ),
    }
}

/// Launch shim. It records its own pid before `exec`, so the pid file names
/// the server and not a wrapper process.
fn launch_script() -> &'static str {
    "#!/bin/sh\n\
     # Generated by wf-server on every start; do not edit.\n\
     root=\"$1\"\n\
     port=\"$2\"\n\
     echo $$ > \"$root/run/wf-server.pid\"\n\
     exec \"$root/current/wf-server\" --addr \"127.0.0.1:$port\" --settings-dir \"$root\" >> \"$root/logs/wf-server.log\" 2>&1\n"
}

fn start_steps(layout: &Layout, port: u16) -> Vec<DeployStepView> {
    vec![
        DeployStepView {
            id: "launch-shim".to_string(),
            description: "Write the pid-recording start shim".to_string(),
            command: format!(
                "cat > {}/run/launch.sh.tmp && chmod 700 {}/run/launch.sh.tmp && mv {}/run/launch.sh.tmp {}/run/launch.sh",
                layout.root, layout.root, layout.root, layout.root
            ),
        },
        DeployStepView {
            id: "spawn".to_string(),
            description: "Detach the server from the ssh channel and start it".to_string(),
            command: format!(
                "if [ -f {b}/run/wf-server.pid ] && kill -0 \"$(cat {b}/run/wf-server.pid)\" 2>/dev/null; then echo already-running; exit 0; fi; \
                 if command -v setsid >/dev/null 2>&1; then setsid sh {b}/run/launch.sh {b} {port} < /dev/null > /dev/null 2>&1 & \
                 else nohup sh {b}/run/launch.sh {b} {port} < /dev/null > /dev/null 2>&1 & fi; \
                 sleep 1; if kill -0 \"$(cat {b}/run/wf-server.pid 2>/dev/null)\" 2>/dev/null; then echo started; else echo start-failed; tail -n 20 {b}/logs/wf-server.log 2>/dev/null; exit 1; fi",
                b = layout.root,
                port = port
            ),
        },
    ]
}

fn health_step(layout: &Layout, port: u16, timeout_secs: u64) -> DeployStepView {
    let attempts = (timeout_secs.max(1)) * 2;
    DeployStepView {
        id: "health".to_string(),
        description: "Wait for the started server to answer its loopback health endpoint"
            .to_string(),
        command: format!(
            "i=0; while [ \"$i\" -lt {attempts} ]; do \
               if command -v curl >/dev/null 2>&1; then \
                 if curl -fsS --max-time 2 'http://127.0.0.1:{port}/health' >/dev/null 2>&1; then echo healthy; exit 0; fi; \
               else \
                 if kill -0 \"$(cat {b}/run/wf-server.pid 2>/dev/null)\" 2>/dev/null; then echo alive; exit 0; fi; \
               fi; \
               if ! kill -0 \"$(cat {b}/run/wf-server.pid 2>/dev/null)\" 2>/dev/null; then echo process-exited; tail -n 20 {b}/logs/wf-server.log 2>/dev/null; exit 1; fi; \
               i=$((i+1)); sleep 0.5; \
             done; echo health-timeout; exit 1",
            b = layout.root,
            port = port,
            attempts = attempts
        ),
    }
}

fn stop_steps(layout: &Layout, port: u16) -> Vec<DeployStepView> {
    vec![DeployStepView {
        id: "stop".to_string(),
        description: "Send a term signal, wait for the port to close, then escalate".to_string(),
        command: format!(
            "pidfile={b}/run/wf-server.pid; \
             if [ -f \"$pidfile\" ]; then pid=$(cat \"$pidfile\" 2>/dev/null); fi; \
             if [ -n \"${{pid:-}}\" ] && kill -0 \"$pid\" 2>/dev/null; then kill \"$pid\" 2>/dev/null; \
               i=0; while [ \"$i\" -lt {polls} ]; do if ! kill -0 \"$pid\" 2>/dev/null; then break; fi; i=$((i+1)); sleep 0.5; done; \
               if kill -0 \"$pid\" 2>/dev/null; then kill -9 \"$pid\" 2>/dev/null; fi; fi; \
             rm -f \"$pidfile\"; \
             if command -v pkill >/dev/null 2>&1; then pkill -f 'wf-server --addr 127.0.0.1:{port}' >/dev/null 2>&1 || true; fi; \
             echo stopped",
            b = layout.root,
            port = port,
            polls = STOP_GRACE_POLLS
        ),
    }]
}

/// Uninstall always removes the whole install root: binaries, settings,
/// scripts, pid file and logs. There is no `purge` flag and no partial
/// cleanup mode — a half-removed deployment is not a useful state, and any
/// finer operation belongs on a direct `ssh` session.
fn uninstall_steps(layout: &Layout) -> Vec<DeployStepView> {
    vec![DeployStepView {
        id: "remove".to_string(),
        description: "Remove the install root with binaries, configuration, scripts and logs"
            .to_string(),
        command: format!("rm -rf {} && echo removed", layout.root),
    }]
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

/// Build the plan for one action. Probing first is deliberate: every step
/// below depends on the remote home, the platform and the current install,
/// so a plan assembled without them could only be a guess.
pub async fn plan(
    backend: &str,
    entry: &BackendEntry,
    settings: &DeploySettings,
    request: &DeployRequest,
    local_port: u16,
) -> Result<DeployPlanView, String> {
    let action = request.parsed_action()?;
    let scope = request.parsed_scope()?;
    let root = normalize_install_root(&settings.install_root)?;
    let probed = probe(backend, entry, settings, local_port).await;
    let target = probed.target.clone().unwrap_or_default();
    if probed.error.is_some() {
        return Ok(with_plan_id(DeployPlanView {
            backend: backend.to_string(),
            action: action.as_str().to_string(),
            install_root: root,
            server_port: entry.effective_server_port(),
            scope: scope.as_str().to_string(),
            source: request.source.clone().unwrap_or_default(),
            target: Some(target),
            blocked: probed.error.clone(),
            planned_ms: wf_common::epoch_ms(),
            ..DeployPlanView::default()
        }));
    }
    let layout = Layout::new(&root);
    let mut files: Vec<PayloadFile> = Vec::new();
    let mut warnings = Vec::new();
    let mut blocked = None;
    let mut artifact_view = None;
    let mut steps = Vec::new();
    match action {
        DeployAction::Probe => {}
        DeployAction::Install | DeployAction::Upgrade => {
            files = read_source_files(settings, request.source.as_deref(), scope)?;
            if files.is_empty() {
                warnings.push(
                    "scope=none: no local files are transferred; the remote keeps its existing settings"
                        .to_string(),
                );
            }
            let resolved = resolve_artifact(&request.artifact, &target, settings)?;
            artifact_view = Some(resolved.view);
            if let Some(reason) = resolved.blk {
                if request.force {
                    warnings.push(format!("{reason} (forced)"));
                } else {
                    blocked = Some(reason);
                }
            }
            if let Some(reason) = download_transport_problem(&resolved.payload, &target) {
                if request.force {
                    warnings.push(format!("{reason} (forced)"));
                } else {
                    blocked = Some(reason);
                }
            }
            steps.extend(install_steps(&layout, &resolved.payload));
            for (path, _, _) in &files {
                steps.push(file_step(&layout, path));
            }
            if action == DeployAction::Upgrade {
                steps.extend(stop_steps(&layout, entry.effective_server_port()));
                steps.extend(start_steps(&layout, entry.effective_server_port()));
                steps.push(health_step(
                    &layout,
                    entry.effective_server_port(),
                    settings.start_timeout_secs,
                ));
            }
            if !target.curl {
                warnings.push(
                    "curl is missing on the remote host; the health wait falls back to a process liveness check"
                        .to_string(),
                );
            }
        }
        DeployAction::Start => {
            if probed
                .installation
                .as_ref()
                .and_then(|i| i.version.as_ref())
                .is_none()
            {
                blocked = Some(
                    "nothing is installed under the install root; run the install action first"
                        .to_string(),
                );
            }
            if probed
                .installation
                .as_ref()
                .map(|i| i.running)
                .unwrap_or(false)
            {
                warnings.push(
                    "a process is already running for this backend; start is a no-op".to_string(),
                );
            }
            steps.extend(start_steps(&layout, entry.effective_server_port()));
            steps.push(health_step(
                &layout,
                entry.effective_server_port(),
                settings.start_timeout_secs,
            ));
        }
        DeployAction::Stop => {
            if !probed
                .installation
                .as_ref()
                .map(|i| i.running)
                .unwrap_or(false)
            {
                warnings
                    .push("no recorded process is running; stop only sweeps leftovers".to_string());
            }
            steps.extend(stop_steps(&layout, entry.effective_server_port()));
        }
        DeployAction::Uninstall => {
            steps.extend(stop_steps(&layout, entry.effective_server_port()));
            steps.extend(uninstall_steps(&layout));
        }
    }
    if scope != DeployScope::None
        && !matches!(action, DeployAction::Install | DeployAction::Upgrade)
    {
        warnings.push(format!(
            "scope={} is ignored for action {}; files travel only with install or upgrade",
            scope.as_str(),
            action.as_str()
        ));
    }
    Ok(with_plan_id(DeployPlanView {
        plan_id: String::new(),
        backend: backend.to_string(),
        action: action.as_str().to_string(),
        install_root: root,
        server_port: entry.effective_server_port(),
        scope: scope.as_str().to_string(),
        source: request.source.clone().unwrap_or_default(),
        target: Some(target),
        installation: probed.installation.clone(),
        artifact: artifact_view,
        files: files
            .iter()
            .map(|(path, bytes, mode)| PlannedFileView {
                path: path.clone(),
                bytes: bytes.len(),
                mode: (*mode).to_string(),
            })
            .collect(),
        steps,
        warnings,
        blocked,
        planned_ms: wf_common::epoch_ms(),
    }))
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

/// Install record written to `install.json`.
#[derive(Debug, Serialize)]
struct InstallRecord<'a> {
    version: String,
    platform: String,
    artifact_kind: &'a str,
    artifact_source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact_sha256: Option<&'a str>,
    installed_ms: i64,
    manager_version: &'a str,
}

/// Run one step and record its result.
async fn run_step(
    channel: &DeployChannel<'_>,
    step: &DeployStepView,
    stdin: Option<Vec<u8>>,
) -> DeployStepResultView {
    match channel.run(&step.command, stdin).await {
        Ok(output) => DeployStepResultView {
            id: step.id.clone(),
            ok: output.success(),
            exit_code: output.status,
            stderr: output.trimmed_stderr(),
            stdout: output.trimmed_stdout(),
        },
        Err(e) => DeployStepResultView {
            id: step.id.clone(),
            ok: false,
            exit_code: None,
            stderr: Some(e),
            stdout: None,
        },
    }
}

/// Build the one terminal shape used when an operator or shutdown stops a
/// job. Every interrupt carries a fresh probe so the view never hides a
/// half-applied remote change.
fn interrupted_outcome(
    backend: &str,
    action: DeployAction,
    root: &str,
    version: Option<String>,
    steps: Vec<DeployStepResultView>,
    probe: Option<DeployProbeView>,
    error: &str,
) -> DeployOutcomeView {
    DeployOutcomeView {
        backend: backend.to_string(),
        action: action.as_str().to_string(),
        ok: false,
        install_root: root.to_string(),
        version,
        steps,
        error: Some(error.to_string()),
        probe,
        finished_ms: wf_common::epoch_ms(),
        status: Some("interrupted".to_string()),
        ..DeployOutcomeView::default()
    }
}

/// Apply one action and re-probe at the end.
///
/// The step list is recomputed here rather than taken from a previous plan
/// request: an approved plan is a description of intent, and the remote
/// state may have moved between the two calls.
pub async fn apply(
    backend: &str,
    entry: &BackendEntry,
    settings: &DeploySettings,
    request: &DeployRequest,
    local_port: u16,
    cancel: &CancellationToken,
    mut on_plan: impl FnMut(&DeployPlanView),
    mut on_progress: impl FnMut(DeployStepProgress),
) -> Result<DeployOutcomeView, String> {
    let action = request.parsed_action()?;
    let root = normalize_install_root(&settings.install_root)?;
    let layout = Layout::new(&root);
    let port = entry.effective_server_port();
    if cancel.is_cancelled() {
        return Ok(interrupted_outcome(
            backend,
            action,
            &root,
            None,
            Vec::new(),
            Some(probe(backend, entry, settings, local_port).await),
            "deployment interrupted",
        ));
    }
    let planned = plan(backend, entry, settings, request, local_port).await?;
    let total_steps = planned.steps.len();
    on_plan(&planned);
    if let Err(e) = require_approved_plan(action, request.plan_id.as_deref(), &planned.plan_id) {
        return Ok(DeployOutcomeView {
            backend: backend.to_string(),
            action: action.as_str().to_string(),
            ok: false,
            install_root: root,
            error: Some(e),
            probe: None,
            finished_ms: wf_common::epoch_ms(),
            status: Some("failed".to_string()),
            ..DeployOutcomeView::default()
        });
    }
    if cancel.is_cancelled() {
        return Ok(interrupted_outcome(
            backend,
            action,
            &root,
            None,
            Vec::new(),
            Some(probe(backend, entry, settings, local_port).await),
            "deployment interrupted",
        ));
    }
    if let Some(reason) = planned.blocked.clone() {
        return Ok(DeployOutcomeView {
            backend: backend.to_string(),
            action: action.as_str().to_string(),
            ok: false,
            install_root: root,
            error: Some(reason),
            probe: Some(probe(backend, entry, settings, local_port).await),
            finished_ms: wf_common::epoch_ms(),
            status: Some("failed".to_string()),
            ..DeployOutcomeView::default()
        });
    }
    let channel = DeployChannel::new(
        entry,
        local_port,
        Duration::from_secs(settings.exec_timeout_secs.max(1)),
    );
    let scope = request.parsed_scope()?;
    let files = match action {
        DeployAction::Install | DeployAction::Upgrade => {
            read_source_files(settings, request.source.as_deref(), scope)?
        }
        _ => Vec::new(),
    };
    let target = planned.target.clone().unwrap_or_default();
    let resolved = match action {
        DeployAction::Install | DeployAction::Upgrade => {
            Some(resolve_artifact(&request.artifact, &target, settings)?)
        }
        _ => None,
    };
    let mut results: Vec<DeployStepResultView> = Vec::new();
    let mut version: Option<String> = None;

    // Every branch runs its steps in plan order and stops at the first
    // failure, so `steps` read as the prefix that succeeded. A cancellation
    // is checked before each step and races each step, so it converges to an
    // `interrupted` outcome rather than a `failed` one. Progress reports the
    // in-flight step before it starts and its result after it settles.
    macro_rules! push_step {
        ($step:expr, $stdin:expr) => {{
            let step = &$step;
            on_progress(DeployStepProgress::running(&step.id, total_steps));
            if cancel.is_cancelled() {
                return Ok(interrupted_outcome(
                    backend,
                    action,
                    &root,
                    version,
                    results,
                    Some(probe(backend, entry, settings, local_port).await),
                    "deployment interrupted",
                ));
            }
            let result = tokio::select! {
                _ = cancel.cancelled() => {
                    return Ok(interrupted_outcome(
                        backend,
                        action,
                        &root,
                        version,
                        results,
                        Some(probe(backend, entry, settings, local_port).await),
                        "deployment interrupted",
                    ));
                }
                result = run_step(&channel, step, $stdin) => result,
            };
            let ok = result.ok;
            results.push(result.clone());
            on_progress(DeployStepProgress::done(result, total_steps));
            if !ok {
                return Ok(DeployOutcomeView {
                    backend: backend.to_string(),
                    action: action.as_str().to_string(),
                    ok: false,
                    install_root: root,
                    version,
                    steps: results,
                    error: Some(format!("step '{}' failed", $step.id)),
                    probe: Some(probe(backend, entry, settings, local_port).await),
                    finished_ms: wf_common::epoch_ms(),
                    status: Some("failed".to_string()),
                    ..DeployOutcomeView::default()
                });
            }
        }};
    }

    if let Some(resolved) = resolved {
        let steps = install_steps(&layout, &resolved.payload);
        // Step 0 creates the layout and needs no payload.
        push_step!(steps[0], None);
        // Step 1 streams the artifact; its stdin is the binary itself.
        push_step!(
            steps[1],
            match &resolved.payload {
                ArtifactPayload::Local { bytes } => Some(bytes.clone()),
                _ => None,
            }
        );
        // Step 2 activates and prints the version it settled on.
        let activate = steps[2].clone();
        push_step!(activate, None);
        if cancel.is_cancelled() {
            return Ok(interrupted_outcome(
                backend,
                action,
                &root,
                version,
                results,
                Some(probe(backend, entry, settings, local_port).await),
                "deployment interrupted",
            ));
        }
        version = probe(backend, entry, settings, local_port)
            .await
            .installation
            .and_then(|i| i.version.clone());
        let record = InstallRecord {
            version: version
                .clone()
                .unwrap_or_else(|| clean_version(env!("CARGO_PKG_VERSION"))),
            platform: target.platform.clone(),
            artifact_kind: &resolved.view.kind,
            artifact_source: &resolved.view.source,
            artifact_sha256: resolved.view.sha256.as_deref(),
            installed_ms: wf_common::epoch_ms(),
            manager_version: env!("CARGO_PKG_VERSION"),
        };
        let record_body = serde_json::to_vec_pretty(&record)
            .map_err(|e| format!("cannot encode the install record: {e}"))?;
        push_step!(steps[3], Some(record_body));
        for (path, bytes, _) in &files {
            let file_step = file_step(&layout, path);
            push_step!(file_step, Some(bytes.clone()));
        }
    }

    match action {
        DeployAction::Install | DeployAction::Probe => {}
        DeployAction::Stop => {
            for step in stop_steps(&layout, port) {
                push_step!(step, None);
            }
        }
        DeployAction::Uninstall => {
            for step in stop_steps(&layout, port) {
                push_step!(step, None);
            }
            for step in uninstall_steps(&layout) {
                push_step!(step, None);
            }
        }
        DeployAction::Start | DeployAction::Upgrade => {
            // An upgrade must stop the old process first: `current` already
            // points at the new version, but a running process keeps the
            // binary it was started from, so skipping this would report
            // success while the old version keeps serving.
            if action == DeployAction::Upgrade {
                for step in stop_steps(&layout, port) {
                    push_step!(step, None);
                }
            }
            for step in start_steps(&layout, port) {
                let stdin = if step.id == "launch-shim" {
                    Some(launch_script().as_bytes().to_vec())
                } else {
                    None
                };
                push_step!(step, stdin);
            }
            let health = health_step(&layout, port, settings.start_timeout_secs);
            push_step!(health, None);
        }
    }

    if cancel.is_cancelled() {
        return Ok(interrupted_outcome(
            backend,
            action,
            &root,
            version,
            results,
            Some(probe(backend, entry, settings, local_port).await),
            "deployment interrupted",
        ));
    }
    if version.is_none() {
        version = probe(backend, entry, settings, local_port)
            .await
            .installation
            .and_then(|i| i.version.clone());
    }
    if cancel.is_cancelled() {
        return Ok(interrupted_outcome(
            backend,
            action,
            &root,
            version,
            results,
            Some(probe(backend, entry, settings, local_port).await),
            "deployment interrupted",
        ));
    }
    Ok(DeployOutcomeView {
        backend: backend.to_string(),
        action: action.as_str().to_string(),
        ok: true,
        install_root: root,
        version,
        steps: results,
        error: None,
        probe: Some(probe(backend, entry, settings, local_port).await),
        finished_ms: wf_common::epoch_ms(),
        status: Some("succeeded".to_string()),
        ..DeployOutcomeView::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
            backup_directory: None,
        }
    }

    #[test]
    fn install_root_stays_relative() {
        assert_eq!(normalize_install_root(".wf-server").unwrap(), ".wf-server");
        assert_eq!(
            normalize_install_root("./.wf-server").unwrap(),
            ".wf-server"
        );
        assert!(normalize_install_root("/etc/wf").is_err());
        assert!(normalize_install_root("~/wf").is_err());
        assert!(normalize_install_root("../outside").is_err());
        assert!(normalize_install_root("a b").is_err());
    }

    #[test]
    fn log_path_stays_under_the_quoted_install_root() {
        let path = log_path(".wf-server");
        assert_eq!(path, "\"$HOME\"/'.wf-server'/logs/wf-server.log");
    }

    #[test]
    fn log_tail_keeps_the_newest_bytes_on_a_char_boundary() {
        let raw = format!("{}{}", "x".repeat(MAX_REMOTE_LOG_BYTES), "é");
        let (text, truncated) = bound_log_tail(&raw);
        assert!(truncated);
        assert!(text.starts_with("..."));
        assert!(text.ends_with('é'));
        assert!(text.len() <= MAX_REMOTE_LOG_BYTES + 3);
    }

    fn target(platform: &str) -> DeployTarget {
        DeployTarget {
            platform: platform.to_string(),
            curl: true,
            ..DeployTarget::default()
        }
    }

    #[test]
    fn download_without_curl_is_reported_before_execution() {
        let mut remote = target("linux-x86_64");
        remote.curl = false;
        let url = ArtifactPayload::Url {
            url: "https://packages.example.test/a.tar.gz".to_string(),
            sha256: None,
            archive: true,
        };
        assert!(download_transport_problem(&url, &remote).is_some());
        remote.curl = true;
        assert!(download_transport_problem(&url, &remote).is_none());
        let local = ArtifactPayload::Local { bytes: vec![1] };
        remote.curl = false;
        assert!(download_transport_problem(&local, &remote).is_none());
    }

    #[test]
    fn source_dirs_cannot_escape_the_source_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("staging/server")).unwrap();
        let settings = DeploySettings {
            source_root: root.path().to_path_buf(),
            ..DeploySettings::default()
        };
        let (dir, name) = resolve_source_dir(&settings, Some("staging")).unwrap();
        assert_eq!(name, "staging");
        assert_eq!(dir, root.path().join("staging"));
        assert!(resolve_source_dir(&settings, Some("..")).is_err());
        assert!(resolve_source_dir(&settings, Some("/etc")).is_err());
        assert!(resolve_source_dir(&settings, Some("missing")).is_err());
        assert_eq!(resolve_source_dir(&settings, None).unwrap().1, "");
    }

    #[test]
    fn scope_selects_only_config_or_config_plus_scripts() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staging");
        std::fs::create_dir_all(source.join("server")).unwrap();
        std::fs::create_dir_all(source.join("scripts")).unwrap();
        std::fs::write(source.join("server/server.toml"), "x = 1\n").unwrap();
        std::fs::write(source.join("server/other.toml"), "y = 2\n").unwrap();
        std::fs::write(source.join("scripts/boot.sh"), "#!/bin/sh\n").unwrap();
        std::fs::write(source.join("server/notes.md"), "ignored\n").unwrap();
        let settings = DeploySettings {
            source_root: root.path().to_path_buf(),
            ..DeploySettings::default()
        };
        let config = read_source_files(&settings, Some("staging"), DeployScope::Config).unwrap();
        assert_eq!(config.len(), 2);
        assert!(config
            .iter()
            .all(|(path, _, _)| path.starts_with("server/")));
        let full = read_source_files(&settings, Some("staging"), DeployScope::Full).unwrap();
        assert_eq!(full.len(), 3);
        assert!(full
            .iter()
            .any(|(path, _, mode)| path == "scripts/boot.sh" && *mode == "700"));
        assert!(
            read_source_files(&settings, Some("staging"), DeployScope::None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn release_artifact_url_matches_the_package_contract() {
        assert_eq!(
            release_artifact_url(
                "https://packages.example.test/builds/",
                "1.2.3",
                "linux-aarch64"
            ),
            "https://packages.example.test/builds/wf-agent-server-1.2.3-linux-aarch64.tar.gz"
        );
    }

    #[test]
    fn scripts_get_executable_mode() {
        assert_eq!(mode_for("scripts/a.sh"), "700");
        assert_eq!(mode_for("server/server.toml"), "600");
    }

    #[test]
    fn probe_parses_records_and_platform() {
        let stdout = "os\tLinux\narch\tamd64\nhome\t/home/dev\nshell\t/bin/bash\ncurl\t1\npath\t/usr/bin/wf-server\nroot\t/home/dev/.wf-server\ncurrent\tbin/0.1.0\nversion\twf-server 0.1.0\nbin\t0.1.0\nbin\t0.1.1\nconf\tserver.toml\nscript\tbootstrap.sh\npid\t4321\nhealthy\t1\n";
        let record = parse_probe(stdout, ".wf-server");
        assert_eq!(record.target.platform, "linux-x86_64");
        assert!(record.target.curl);
        assert_eq!(
            record.target.wf_server_on_path.as_deref(),
            Some("/usr/bin/wf-server")
        );
        let install = record.installation.expect("installed");
        assert_eq!(install.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            install.versions,
            vec!["0.1.0".to_string(), "0.1.1".to_string()]
        );
        assert!(install.running);
        assert!(install.healthy);
        assert_eq!(install.pid, Some(4321));
    }

    #[test]
    fn absent_root_reports_no_installation() {
        let record = parse_probe(
            "os\tDarwin\narch\tarm64\nhome\t/Users/dev\ncurl\t0\n",
            ".wf-server",
        );
        assert_eq!(record.target.platform, "darwin-aarch64");
        assert!(record.installation.is_none());
    }

    #[test]
    fn action_parser_names_valid_verbs() {
        assert_eq!(
            DeployAction::parse("upgrade").unwrap(),
            DeployAction::Upgrade
        );
        assert!(DeployAction::parse("rm -rf /").is_err());
        assert!(!DeployAction::Probe.writes());
        assert!(DeployAction::Install.writes());
    }

    #[test]
    fn release_download_is_the_default_artifact_source() {
        let settings = DeploySettings::default();
        let resolved = resolve_artifact(&None, &target("linux-aarch64"), &settings).unwrap();
        assert_eq!(resolved.view.kind, "release");
        assert!(resolved.blk.is_none(), "{:?}", resolved.blk);
        assert!(resolved.view.source.contains("wf-agent-server-"));
        assert!(resolved.view.source.contains("linux-aarch64.tar.gz"));
        assert!(matches!(
            resolved.payload,
            ArtifactPayload::Url { archive: true, .. }
        ));
    }

    #[test]
    fn empty_base_url_blocks_the_release_source() {
        let settings = DeploySettings {
            artifact_base_url: String::new(),
            ..DeploySettings::default()
        };
        let resolved = resolve_artifact(&None, &target("linux-x86_64"), &settings).unwrap();
        assert!(resolved.blk.is_some());
        assert_eq!(resolved.view.status, "missing_base_url");
    }

    #[test]
    fn explicit_local_artifact_still_checks_the_platform() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("wf-server");
        std::fs::write(&artifact, b"local artifact").unwrap();
        let spec = Some(ArtifactSpec {
            kind: "local".to_string(),
            path: artifact.display().to_string(),
            version: None,
            sha256: None,
        });
        let settings = DeploySettings::default();
        let resolved = resolve_artifact(&spec, &target("darwin-aarch64"), &settings).unwrap();
        if local_platform() != "darwin-aarch64" {
            assert!(resolved.blk.is_some());
            assert_eq!(resolved.view.status, "platform_mismatch");
        }
    }

    #[test]
    fn unknown_artifact_kind_is_rejected() {
        let spec = Some(ArtifactSpec {
            kind: "ftp".to_string(),
            path: "x".to_string(),
            version: None,
            sha256: None,
        });
        assert!(
            resolve_artifact(&spec, &target("linux-x86_64"), &DeploySettings::default()).is_err()
        );
    }

    #[test]
    fn install_plan_uses_versioned_directories() {
        let layout = Layout::new(".wf-server");
        let steps = install_steps(&layout, &ArtifactPayload::Local { bytes: vec![1] });
        assert_eq!(steps[0].id, "layout");
        assert!(steps[0].command.contains("mkdir -p"));
        assert!(steps[1].command.contains("upload.tmp"));
        assert!(steps[2].command.contains("ln -sfn bin/$v"));
    }

    #[test]
    fn stop_matches_the_exact_listen_address() {
        let layout = Layout::new(".wf-server");
        let steps = stop_steps(&layout, 4321);
        assert!(steps[0].command.contains("127.0.0.1:4321"));
        assert!(steps[0].command.contains("kill -9"));
    }

    #[test]
    fn payload_limits_reject_oversized_source_files() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().to_path_buf();
        std::fs::create_dir_all(source.join("server")).unwrap();
        std::fs::write(
            source.join("server/server.toml"),
            "x".repeat(MAX_FILE_BYTES + 1),
        )
        .unwrap();
        let settings = DeploySettings {
            source_root: source.clone(),
            ..DeploySettings::default()
        };
        assert!(read_source_files(&settings, None, DeployScope::Config).is_err());
        std::fs::write(source.join("server/server.toml"), "enabled = true\n").unwrap();
        assert_eq!(
            read_source_files(&settings, None, DeployScope::Config)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn uninstall_removes_the_whole_install_root() {
        let layout = Layout::new(".wf-server");
        let steps = uninstall_steps(&layout);
        assert_eq!(steps.len(), 1);
        assert!(steps[0].command.contains("rm -rf"));
        // The root directory is removed once: binaries, settings, scripts,
        // pid file and logs all go with it. No keep/partial variant exists.
        assert_eq!(
            steps[0].command,
            format!("rm -rf {} && echo removed", layout.root)
        );
    }

    #[test]
    fn launch_script_never_passes_project_config() {
        // The project layer means one workspace's `.wf/` overrides. A
        // service instance, remote or local, must not bind to one workspace.
        let script = launch_script();
        assert!(script.contains("--settings-dir"));
        assert!(!script.contains("--config"));
    }

    #[test]
    fn probe_verdict_reflects_missing_pieces() {
        let good = RemoteInstallation {
            current: Some("bin/0.1.0".to_string()),
            version: Some("0.1.0".to_string()),
            dirs: ["bin", "current", "server", "scripts"]
                .iter()
                .map(|value| value.to_string())
                .collect(),
            running: true,
            healthy: true,
            config_files: vec!["server.toml".to_string()],
            script_files: vec!["bootstrap.sh".to_string()],
            ..RemoteInstallation::default()
        };
        let (verdict, checks) = evaluate_probe(Some(&good), &target("linux-x86_64"));
        assert_eq!(verdict, "ok");
        assert!(checks.iter().all(|check| check.ok));
        let stopped = RemoteInstallation {
            running: false,
            ..good
        };
        let (verdict, checks) = evaluate_probe(Some(&stopped), &target("linux-x86_64"));
        assert_eq!(verdict, "degraded");
        assert!(
            !checks
                .iter()
                .find(|check| check.id == "running")
                .unwrap()
                .ok
        );
        let (verdict, _) = evaluate_probe(None, &target("linux-x86_64"));
        assert_eq!(verdict, "absent");
    }

    #[test]
    fn scope_parser_accepts_only_three_shapes() {
        assert_eq!(DeployScope::parse("").unwrap(), DeployScope::None);
        assert_eq!(DeployScope::parse("config").unwrap(), DeployScope::Config);
        assert_eq!(DeployScope::parse("full").unwrap(), DeployScope::Full);
        assert!(DeployScope::parse("server/server.toml").is_err());
    }

    #[test]
    fn ssh_argv_reuses_the_tunnel_control_socket() {
        let pinned = entry();
        let channel = DeployChannel::new(&pinned, 18080, Duration::from_secs(5));
        let argv = channel.ssh_argv("true");
        assert_eq!(argv.first().map(String::as_str), Some("ssh"));
        assert!(argv.iter().any(|part| part == "BatchMode=yes"));
        assert!(argv.iter().any(|part| part.starts_with("ControlPath=")));
        assert_eq!(argv.last().map(String::as_str), Some("true"));
        assert!(argv.contains(&"dev@example".to_string()));
    }

    #[test]
    fn shell_quote_survives_single_quotes() {
        assert_eq!(shell_quote("/srv/app"), "'/srv/app'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    fn sample_plan() -> DeployPlanView {
        with_plan_id(DeployPlanView {
            plan_id: String::new(),
            backend: "prod".to_string(),
            action: "upgrade".to_string(),
            install_root: ".wf-server".to_string(),
            server_port: 3000,
            scope: "config".to_string(),
            source: "staging".to_string(),
            target: Some(target("linux-x86_64")),
            installation: None,
            artifact: Some(ArtifactPlanView {
                kind: "release".to_string(),
                source: "https://example.test/a.tar.gz".to_string(),
                bytes: Some(128),
                sha256: None,
                status: "ok".to_string(),
            }),
            files: vec![PlannedFileView {
                path: "server/server.toml".to_string(),
                bytes: 12,
                mode: "600".to_string(),
            }],
            steps: vec![DeployStepView {
                id: "layout".to_string(),
                description: "Create layout".to_string(),
                command: "mkdir -p ~/.wf-server".to_string(),
            }],
            warnings: Vec::new(),
            blocked: None,
            planned_ms: 100,
        })
    }

    #[test]
    fn plan_id_ignores_timestamps_but_tracks_execution_content() {
        let base = sample_plan();
        let mut later = base.clone();
        later.planned_ms += 60_000;
        assert_eq!(plan_fingerprint(&base), plan_fingerprint(&later));

        let mut changed_command = base.clone();
        changed_command.steps[0]
            .command
            .push_str(" && echo changed");
        assert_ne!(plan_fingerprint(&base), plan_fingerprint(&changed_command));

        let mut changed_file = base.clone();
        changed_file.files[0].bytes += 1;
        assert_ne!(plan_fingerprint(&base), plan_fingerprint(&changed_file));

        let mut changed_platform = base.clone();
        changed_platform.target = Some(target("darwin-aarch64"));
        assert_ne!(plan_fingerprint(&base), plan_fingerprint(&changed_platform));
    }

    #[test]
    fn approval_must_match_the_recomputed_plan_id() {
        let plan = sample_plan();
        assert!(
            require_approved_plan(DeployAction::Upgrade, Some(&plan.plan_id), &plan.plan_id)
                .is_ok()
        );
        assert!(
            require_approved_plan(DeployAction::Upgrade, Some("stale"), &plan.plan_id).is_err()
        );
        assert!(require_approved_plan(DeployAction::Upgrade, None, &plan.plan_id).is_err());
        assert!(require_approved_plan(DeployAction::Probe, None, &plan.plan_id).is_ok());
        // An explicit override never turns an invalid approval into a valid one.
        let forced = DeployRequest {
            action: "upgrade".to_string(),
            force: true,
            plan_id: Some("stale".to_string()),
            ..DeployRequest::default()
        };
        assert!(require_approved_plan(
            DeployAction::Upgrade,
            forced.plan_id.as_deref(),
            &plan.plan_id
        )
        .is_err());
    }

    #[tokio::test]
    async fn plan_drift_fails_before_any_remote_step() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake ssh port");
        let port = listener.local_addr().expect("bound port").port();
        let held = tokio::spawn(async move {
            let mut sockets = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                sockets.push(socket);
            }
        });
        let mut remote = entry();
        remote.host = "127.0.0.1".to_string();
        remote.port = Some(port);
        let settings = DeploySettings {
            exec_timeout_secs: 1,
            ..DeploySettings::default()
        };
        let request = DeployRequest {
            action: "stop".to_string(),
            plan_id: Some("stale".to_string()),
            ..DeployRequest::default()
        };
        let mut plans = Vec::new();
        let outcome = apply(
            "prod",
            &remote,
            &settings,
            &request,
            18080,
            &CancellationToken::new(),
            |plan| plans.push(plan.clone()),
            |_| {},
        )
        .await
        .expect("apply returns a failed outcome");
        held.abort();

        assert_eq!(outcome.status.as_deref(), Some("failed"));
        assert!(outcome
            .error
            .as_deref()
            .is_some_and(|e| e.contains("run deploy plan again")));
        assert!(outcome.steps.is_empty());
        assert_eq!(plans.len(), 1);
    }

    #[tokio::test]
    async fn an_unconsumed_stdin_payload_times_out() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, _reader) = tokio::io::duplex(16);
        let payload = vec![b'x'; 64 * 1024];
        let output = run_bounded(Duration::from_millis(50), async {
            writer
                .write_all(&payload)
                .await
                .map_err(|e| e.to_string())?;
            Ok(ExecOutput {
                status: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
            })
        })
        .await;
        assert!(output.timed_out);
        assert!(output
            .trimmed_stderr()
            .is_some_and(|e| e.contains("exceeded")));
    }
}
