//! Live state of the code-context service.
//!
//! The config describes what the service *should* be; this type records what
//! it currently *is*: endpoint, credential outcome, capability handshake, last
//! health verdict, the workspace project and index position, and the
//! supervisor position. Consumers ask it per call scope instead of copying a
//! config snapshot, so a service that starts late, dies, recovers, loses one
//! subsystem or is still indexing is observed instead of frozen.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::transport::{
    breaker_for, http_client, probe_health, BreakerSnapshot, HealthReport, SidecarStatus,
    SidecarStatusHandle, TransportMode,
};

pub use crate::transport::CallScope;

use super::dead_letters::{self, DeadLetterAction, DeadLetterList};
use super::project::{
    find_by_root, list_projects, now_ms, start_watch, trigger_full_index, IndexSupplyMode,
    ProjectState, ProjectStatus,
};
use super::{CodeContextConfig, ProjectPolicy};

/// How long a health verdict stays valid before a call re-probes.
const HEALTH_MAX_AGE_MS: u64 = 15_000;
/// Timeout of one health probe.
const HEALTH_PROBE_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug, Default)]
struct HealthState {
    report: Option<HealthReport>,
    probed_at: Option<Instant>,
    /// Terminal blocker: the managed process could not be started.
    startup_error: Option<String>,
    /// Terminal blocker: the declared credential cannot be resolved.
    credential_error: Option<String>,
}

/// Outcome of the one-shot capability handshake.
///
/// CCE has no version endpoint, so compatibility is decided by capability:
/// the health payload must decode into subsystem verdicts and the project
/// endpoint must exist. Either failure means the deployed service is not the
/// contract this adapter speaks, which is reported once instead of per call.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    pub compatible: bool,
    /// Missing capabilities when incompatible.
    pub missing: Vec<String>,
    /// Wall-clock milliseconds of the check.
    pub checked_at_ms: u64,
}

/// What the live service can serve right now, without probing.
///
/// The rule is "hide only what is confirmed unusable": an absent health
/// verdict is not evidence of degradation, so tools stay visible until the
/// service actually reports the subsystem down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CodeContextAvailability {
    pub fold: bool,
    pub navigation: bool,
    pub keyword: bool,
    pub semantic: bool,
}

impl CodeContextAvailability {
    /// Nothing is reachable.
    pub fn none() -> Self {
        Self {
            fold: false,
            navigation: false,
            keyword: false,
            semantic: false,
        }
    }

    /// Whether this availability covers one call scope.
    pub fn covers(&self, scope: CallScope) -> bool {
        match scope {
            CallScope::Fold => self.fold,
            CallScope::Navigation => self.navigation,
            CallScope::KeywordSearch => self.keyword,
            CallScope::HybridSearch => self.semantic,
            // Maintenance needs the process, nothing else. Folding is the
            // process-level channel (parser only), so it is the exact answer
            // to "the service is alive and speaks to us".
            CallScope::Operations => self.fold,
        }
    }
}

/// Point-in-time service state for diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct CodeContextStatus {
    pub enabled: bool,
    pub transport_mode: TransportMode,
    /// Supply mode: who owns the project and its index.
    pub supply_mode: IndexSupplyMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_age_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<CapabilityReport>,
    pub project: ProjectStatus,
    pub availability: CodeContextAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breaker: Option<BreakerSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<SidecarStatus>,
}

/// Failure of one operational (maintenance) call.
///
/// Unlike a query, maintenance is decided locally first: the wrong supply
/// mode or a project that is already indexing cannot be fixed by retrying, so
/// those are reported as [`OperationError::Rejected`] instead of being
/// flattened into "the service failed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationError {
    /// The request cannot be served by construction: the caller must change
    /// something (config, timing, or the requested action).
    Rejected(String),
    /// The service could not be reached or refused the request.
    Service(String),
}

impl OperationError {
    /// Reason text for logs and transport messages.
    pub fn reason(&self) -> &str {
        match self {
            OperationError::Rejected(reason) | OperationError::Service(reason) => reason,
        }
    }

    /// Flatten into the reason string.
    pub fn into_reason(self) -> String {
        match self {
            OperationError::Rejected(reason) | OperationError::Service(reason) => reason,
        }
    }
}

impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

/// Result of applying a new config to a live service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconfigureOutcome {
    /// The managed process must be stopped and started again because the
    /// address or the launch line changed. Addresses are never rewritten in
    /// place: every consumer holds the address it was baked with.
    pub restart_managed: bool,
    /// Why a restart is required, for the caller's log.
    pub reason: Option<String>,
}

/// Shared handle to the code-context service state.
#[derive(Debug)]
pub struct CodeContextService {
    config: RwLock<CodeContextConfig>,
    endpoint: RwLock<Option<String>>,
    /// Workspace root the project belongs to (bootstrap `project_root`,
    /// falling back to the process working directory).
    workspace_root: Option<PathBuf>,
    client: RwLock<Result<reqwest::Client, String>>,
    health: RwLock<HealthState>,
    probe: tokio::sync::Mutex<()>,
    capabilities: RwLock<Option<CapabilityReport>>,
    capability_gate: tokio::sync::Mutex<()>,
    project: RwLock<ProjectState>,
    project_gate: tokio::sync::Mutex<()>,
    sidecar: Mutex<Option<SidecarStatusHandle>>,
}

impl CodeContextService {
    /// Disabled service: every consumer observes "not configured".
    pub fn disabled() -> Self {
        Self::build(CodeContextConfig::default(), None)
    }

    /// External transport: the endpoint is the configured base URL.
    pub fn external(config: CodeContextConfig) -> Self {
        let endpoint = config.transport.normalized_base_url();
        Self::build(config, endpoint)
    }

    /// Managed transport: the endpoint is the loopback address decided
    /// before the process starts, so it stays valid across restarts.
    pub fn managed(config: CodeContextConfig, endpoint: String) -> Self {
        Self::build(config, Some(endpoint))
    }

    /// Managed transport whose address could not be resolved.
    pub fn unresolved(config: CodeContextConfig) -> Self {
        Self::build(config, None)
    }

    /// Attach the workspace root the supplied project belongs to.
    pub fn with_workspace_root(mut self, root: Option<PathBuf>) -> Self {
        self.workspace_root = root;
        self
    }

    fn build(config: CodeContextConfig, endpoint: Option<String>) -> Self {
        let timeout_ms = config.transport.timeout_ms;
        let (client, credential_error) = match config.transport.resolve_auth_token() {
            Ok(token) => {
                // Channels that only hold a baked base URL (compression fold)
                // authenticate through the shared client registry.
                crate::transport::register_service_credential(token.as_deref());
                (http_client(timeout_ms, token.as_deref()), None)
            }
            Err(error) => {
                crate::transport::register_service_credential(None);
                (http_client(timeout_ms, None), Some(error))
            }
        };
        let service = Self::empty(config, endpoint, client);
        if let Some(error) = credential_error {
            service.health_write().credential_error = Some(error);
        }
        service
    }

    fn empty(
        config: CodeContextConfig,
        endpoint: Option<String>,
        client: Result<reqwest::Client, String>,
    ) -> Self {
        let endpoint = if config.enabled { endpoint } else { None };
        let supply = config.project.supply;
        Self {
            config: RwLock::new(config),
            endpoint: RwLock::new(endpoint),
            workspace_root: None,
            client: RwLock::new(client),
            health: RwLock::new(HealthState::default()),
            probe: tokio::sync::Mutex::new(()),
            capabilities: RwLock::new(None),
            capability_gate: tokio::sync::Mutex::new(()),
            project: RwLock::new(match supply {
                IndexSupplyMode::Managed => ProjectState::Unresolved {
                    reason: "project resolution has not run yet".to_string(),
                },
                IndexSupplyMode::External => ProjectState::NotSupplied,
            }),
            project_gate: tokio::sync::Mutex::new(()),
            sidecar: Mutex::new(None),
        }
    }

    /// Policy and transport config backing this service.
    pub fn config(&self) -> CodeContextConfig {
        self.config_read().clone()
    }

    /// Whether the service is switched on.
    pub fn is_enabled(&self) -> bool {
        self.config_read().enabled
    }

    /// Endpoint currently in use, when the transport resolved to one.
    pub fn endpoint(&self) -> Option<String> {
        self.endpoint_read().clone()
    }

    /// Shared HTTP client (connection pooling across calls, default
    /// authorization header when a credential is declared).
    pub fn client(&self) -> Result<reqwest::Client, String> {
        match &*self.client_read() {
            Ok(client) => Ok(client.clone()),
            Err(error) => Err(error.clone()),
        }
    }

    /// Attach the supervisor status handle for diagnostics.
    pub fn attach_sidecar(&self, handle: SidecarStatusHandle) {
        self.lock_sidecar().replace(handle);
    }

    /// Record why the managed process could not be started. The service
    /// stays unavailable until the runtime is reconfigured with a working
    /// binary, so callers fail fast instead of paying per-call timeouts.
    pub fn record_startup_error(&self, reason: impl Into<String>) {
        self.health_write().startup_error = Some(reason.into());
    }

    /// Resolve the endpoint for one call scope, probing health when the last
    /// verdict is stale and running the one-shot capability handshake.
    pub async fn endpoint_for(&self, scope: CallScope) -> Result<String, String> {
        let config = self.config();
        if !config.enabled {
            return Err("code-context service is disabled".to_string());
        }
        let Some(endpoint) = self.endpoint() else {
            return Err("code-context service endpoint is unresolved".to_string());
        };
        {
            let state = self.health_read();
            if let Some(error) = state.startup_error.clone() {
                return Err(format!("code-context service failed to start: {error}"));
            }
            if let Some(error) = state.credential_error.clone() {
                return Err(error);
            }
        }
        let report = self.ensure_health(&endpoint).await?;
        if !report.reachable {
            return Err(format!("code-context service at {endpoint} is unreachable"));
        }
        self.ensure_capabilities(&endpoint).await?;
        if !scope.covered_by(&report) {
            return Err(format!(
                "code-context service cannot serve the {} right now",
                scope.requirement()
            ));
        }
        Ok(endpoint)
    }

    /// Force a health probe, refreshing the cached verdict.
    pub async fn probe(&self) -> Result<HealthReport, String> {
        let Some(endpoint) = self.endpoint() else {
            return Err("code-context service endpoint is unresolved".to_string());
        };
        self.probe_endpoint(&endpoint).await
    }

    /// Point-in-time state for diagnostics and logging. Every cached value is
    /// copied out before asking for the derived availability, so no lock is
    /// held across a nested read.
    pub fn status(&self) -> CodeContextStatus {
        let availability = self.availability();
        let endpoint = self.endpoint();
        let capabilities = self.capabilities_read().clone();
        let project = ProjectStatus::from(&*self.project_read());
        let (enabled, transport_mode, supply_mode) = {
            let config = self.config_read();
            (
                config.enabled,
                config.transport.transport_mode,
                config.project.supply,
            )
        };
        let (startup_error, credential_error, health, health_age_ms) = {
            let state = self.health_read();
            (
                state.startup_error.clone(),
                state.credential_error.clone(),
                state.report,
                state.probed_at.map(|at| at.elapsed().as_millis() as u64),
            )
        };
        CodeContextStatus {
            enabled,
            transport_mode,
            supply_mode,
            breaker: endpoint.as_deref().map(|e| breaker_for(e).snapshot()),
            endpoint,
            startup_error,
            credential_error,
            health,
            health_age_ms,
            capabilities,
            project,
            availability,
            sidecar: self.sidecar_snapshot(),
        }
    }

    /// Capability view for tool-set assembly. Reads cached state only: the
    /// assembly path neither probes nor mutates the breaker.
    pub fn availability(&self) -> CodeContextAvailability {
        let config = self.config_read();
        if !config.enabled {
            return CodeContextAvailability::none();
        }
        let state = self.health_read();
        if state.startup_error.is_some() || state.credential_error.is_some() {
            return CodeContextAvailability::none();
        }
        if self.endpoint_read().is_none() {
            return CodeContextAvailability::none();
        }
        if let Some(capability) = self.capabilities_read().as_ref() {
            if !capability.compatible {
                return CodeContextAvailability::none();
            }
        }
        let mut availability = match state.report {
            None => CodeContextAvailability {
                fold: true,
                navigation: true,
                keyword: true,
                semantic: true,
            },
            Some(report) => CodeContextAvailability {
                fold: report.reachable,
                navigation: report.reachable,
                keyword: report.reachable && report.bm25,
                semantic: report.reachable && report.embedding,
            },
        };
        match &*self.project_read() {
            // A failed supply makes every project-scoped channel fail by
            // construction; folding needs no project and stays visible.
            ProjectState::Failed { .. } => {
                availability.navigation = false;
                availability.keyword = false;
                availability.semantic = false;
            }
            // While the first index runs, keyword and semantic results would
            // be silently incomplete; navigation over the already parsed
            // subset stays useful.
            ProjectState::Indexing { .. } => {
                availability.keyword = false;
                availability.semantic = false;
            }
            _ => {}
        }
        availability
    }

    /// Resolve the project for one call: the call's own id, the runtime
    /// supply, or the configured default. The order keeps an explicit caller
    /// intent first and makes a missing project a project-state problem
    /// rather than a bare number.
    pub async fn project_id_for(self: &Arc<Self>, explicit: Option<i64>) -> Result<i64, String> {
        if let Some(id) = explicit {
            return Ok(id);
        }
        let config = self.config();
        if config.project.supply == IndexSupplyMode::External {
            return config.retrieval.default_project_id.ok_or_else(|| {
                "code-context has no project: the call names none, runtime supply is external and no default_project_id is configured"
                    .to_string()
            });
        }
        match self.project_state() {
            ProjectState::Ready { id, .. } => Ok(id),
            ProjectState::Indexing { id, .. } => Err(format!(
                "code-context project {id} is still indexing the workspace; retrieval would be incomplete. Retry once the index finishes"
            )),
            ProjectState::Failed { reason, .. } => Err(format!(
                "code-context project is unavailable: {reason}; the index is not retried automatically, use the reindex action"
            )),
            ProjectState::NotSupplied | ProjectState::Unresolved { .. }
            | ProjectState::Resolving => {
                if !config.enabled {
                    return Err("code-context service is disabled".to_string());
                }
                // Lazy supply: a service that was unreachable at bootstrap
                // recovers on the next call instead of needing a restart.
                self.supply().await?;
                match self.project_state() {
                    ProjectState::Ready { id, .. } => Ok(id),
                    ProjectState::Indexing { id, .. } => Err(format!(
                        "code-context project {id} is indexing the workspace for the first time; retrieval would be incomplete. Retry once the index finishes"
                    )),
                    other => Err(format!(
                        "code-context project is not ready (state: {}); see the service status",
                        other.name()
                    )),
                }
            }
        }
    }

    /// Point-in-time project state.
    pub fn project_state(&self) -> ProjectState {
        self.project_read().clone()
    }

    /// Resolve the workspace project and queue the first index when the
    /// service has never indexed it. Returns once the decision is made: the
    /// index itself runs on a detached task, because it embeds the whole
    /// workspace and can take minutes.
    pub async fn supply(self: &Arc<Self>) -> Result<(), String> {
        let config = self.config();
        if !config.enabled {
            return Err("code-context service is disabled".to_string());
        }
        if config.project.supply != IndexSupplyMode::Managed {
            return Ok(());
        }
        let project = self.resolve_project().await?;
        if project.1.is_some() {
            self.set_project(ProjectState::Ready {
                id: project.0,
                last_indexed: project.1,
            });
            self.start_watch(project.0).await;
            return Ok(());
        }
        self.set_project(ProjectState::Indexing {
            id: project.0,
            started_at_ms: now_ms(),
        });
        let service = Arc::clone(self);
        tokio::spawn(async move {
            service.run_full_index(project.0).await;
        });
        Ok(())
    }

    /// Apply a new config to the live service: replace policy and transport,
    /// rebuild the client and credential, reset every cached verdict and the
    /// breaker, and report whether the managed process must be restarted.
    ///
    /// The trigger (file change or management endpoint) is deliberately not
    /// part of this seam: hot reload arrives with the general config reload,
    /// so this section never grows a second mechanism of its own.
    pub async fn apply_config(&self, config: CodeContextConfig) -> ReconfigureOutcome {
        let previous = self.config();
        let endpoint = match config.transport.transport_mode {
            TransportMode::External => config.transport.normalized_base_url(),
            // The managed address is decided by the runtime before launch and
            // never rewritten here; a changed launch line reports a restart.
            TransportMode::Managed => self.endpoint(),
        };
        let token = config.transport.resolve_auth_token();
        let restart_managed = managed_restart_reason(&previous, &config);
        {
            let mut slot = self.config_write();
            *slot = config.clone();
        }
        {
            let mut slot = self.endpoint_write();
            *slot = if config.enabled { endpoint } else { None };
        }
        {
            let mut slot = self.client_write();
            *slot = match &token {
                Ok(token) => {
                    crate::transport::register_service_credential(token.as_deref());
                    http_client(config.transport.timeout_ms, token.as_deref())
                }
                Err(error) => {
                    crate::transport::register_service_credential(None);
                    Err(error.clone())
                }
            };
        }
        let mut state = self.health_write();
        state.report = None;
        state.probed_at = None;
        state.startup_error = None;
        state.credential_error = token.err();
        drop(state);
        self.capabilities_write().take();
        self.set_project(match config.project.supply {
            IndexSupplyMode::Managed => ProjectState::Unresolved {
                reason: "project resolution has not run yet".to_string(),
            },
            IndexSupplyMode::External => ProjectState::NotSupplied,
        });
        let mut revalidated: Vec<String> = Vec::new();
        if let Some(url) = previous.transport.base_url.as_deref() {
            revalidated.push(url.to_string());
        }
        if let Some(url) = self.endpoint() {
            revalidated.push(url);
        }
        for url in revalidated {
            breaker_for(&url).reset();
        }
        ReconfigureOutcome {
            restart_managed: restart_managed.is_some(),
            reason: restart_managed,
        }
    }

    /// Workspace root the supplied project belongs to.
    pub fn workspace_root(&self) -> Option<&std::path::Path> {
        self.workspace_root.as_deref()
    }

    /// Install the address of a freshly started managed process. Only the
    /// runtime calls this, and only when it replaced the process: addresses
    /// are never rewritten under a live consumer.
    pub fn set_managed_endpoint(&self, endpoint: String) {
        {
            let mut slot = self.endpoint_write();
            *slot = Some(endpoint.clone());
        }
        let mut state = self.health_write();
        state.report = None;
        state.probed_at = None;
        state.startup_error = None;
        drop(state);
        self.capabilities_write().take();
        breaker_for(&endpoint).reset();
    }

    /// Whether this service normally supplies the workspace project.
    pub fn supplies_project(&self) -> bool {
        self.is_enabled() && self.config().project.supply == IndexSupplyMode::Managed
    }

    /// Policy block owning project supply and watch behavior.
    pub fn project_policy(&self) -> ProjectPolicy {
        self.config().project
    }

    async fn resolve_project(&self) -> Result<(i64, Option<String>), String> {
        let _guard = self.project_gate.lock().await;
        if let ProjectState::Ready { id, last_indexed } = self.project_state() {
            return Ok((id, last_indexed));
        }
        let Some(root) = self.workspace_root.clone() else {
            let reason = "no workspace root is known for the code-context project".to_string();
            self.set_project(ProjectState::Unresolved {
                reason: reason.clone(),
            });
            return Err(reason);
        };
        self.set_project(ProjectState::Resolving);
        match self.list_or_create(&root).await {
            Ok(project) => Ok((project.id, project.last_indexed)),
            Err(error) => {
                self.set_project(ProjectState::Unresolved {
                    reason: error.clone(),
                });
                Err(error)
            }
        }
    }

    async fn list_or_create(
        &self,
        root: &std::path::Path,
    ) -> Result<super::project::ProjectRecord, String> {
        let endpoint = self.endpoint().ok_or_else(|| {
            "code-context service endpoint is unresolved; the project cannot be resolved"
                .to_string()
        })?;
        let client = self.client()?;
        let timeout_ms = self.config().transport.timeout_ms;
        let projects = list_projects(&client, &endpoint, timeout_ms)
            .await
            .map_err(|e| e.reason)?;
        if let Some(project) = find_by_root(&projects, root) {
            return Ok(project.clone());
        }
        let normalizer = super::project::normalize_root_path;
        let normalized = normalizer(root);
        super::project::create_project(&client, &endpoint, timeout_ms, &normalized)
            .await
            .map_err(|e| e.reason)
    }

    /// Run one full index of the project and record the outcome. Failures are
    /// terminal for this run: the state keeps the reason and the project id,
    /// and only a manual reindex starts the next attempt.
    async fn run_full_index(self: Arc<Self>, project_id: i64) {
        let Some(endpoint) = self.endpoint() else {
            self.set_project(ProjectState::Failed {
                reason: "code-context service endpoint is unresolved before indexing".to_string(),
                id: Some(project_id),
            });
            return;
        };
        let client = match self.client() {
            Ok(client) => client,
            Err(error) => {
                self.set_project(ProjectState::Failed {
                    reason: error,
                    id: Some(project_id),
                });
                return;
            }
        };
        match trigger_full_index(&client, &endpoint, project_id).await {
            Ok(()) => {
                let last_indexed = self
                    .refresh_last_indexed(&client, &endpoint, project_id)
                    .await;
                self.set_project(ProjectState::Ready {
                    id: project_id,
                    last_indexed,
                });
                tracing::info!(project_id, "code-context full index finished");
                self.start_watch(project_id).await;
            }
            Err(error) => {
                tracing::warn!(project_id, reason = %error.reason, "code-context full index failed");
                self.set_project(ProjectState::Failed {
                    reason: format!("full index failed: {}", error.reason),
                    id: Some(project_id),
                });
            }
        }
    }

    async fn refresh_last_indexed(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        project_id: i64,
    ) -> Option<String> {
        let timeout_ms = self.config().transport.timeout_ms;
        let projects = list_projects(client, endpoint, timeout_ms).await.ok()?;
        projects
            .into_iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.last_indexed)
    }

    /// Read the project's dead letters.
    ///
    /// Maintenance must work exactly when retrieval does not: while the first
    /// index runs, or after it failed. The operations scope needs the service
    /// process, never a subsystem, and the project id survives a failure.
    pub async fn dead_letters(self: &Arc<Self>) -> Result<DeadLetterList, OperationError> {
        let (endpoint, client, project_id) = self.operation_context().await?;
        let timeout_ms = self.config().transport.timeout_ms;
        dead_letters::list(&client, &endpoint, project_id, timeout_ms)
            .await
            .map_err(|error| OperationError::Service(error.reason))
    }

    /// Retry dead letters: `files` empty retries every candidate.
    pub async fn retry_dead_letters(
        self: &Arc<Self>,
        files: Vec<String>,
    ) -> Result<DeadLetterAction, OperationError> {
        let (endpoint, client, project_id) = self.operation_context().await?;
        dead_letters::retry(&client, &endpoint, project_id, &files)
            .await
            .map_err(|error| OperationError::Service(error.reason))
    }

    /// Acknowledge one dead letter, for one module when named.
    pub async fn acknowledge_dead_letter(
        self: &Arc<Self>,
        file_path: String,
        module: Option<String>,
    ) -> Result<DeadLetterAction, OperationError> {
        let (endpoint, client, project_id) = self.operation_context().await?;
        let timeout_ms = self.config().transport.timeout_ms;
        dead_letters::acknowledge(
            &client,
            &endpoint,
            project_id,
            &file_path,
            module.as_deref(),
            timeout_ms,
        )
        .await
        .map_err(|error| OperationError::Service(error.reason))
    }

    /// Run the full index again on operator request. This is the manual retry
    /// the state machine deliberately does not perform on its own: a failed
    /// index stays failed until someone decides the cause is gone.
    pub async fn retry_index(self: &Arc<Self>) -> Result<i64, OperationError> {
        if !self.supplies_project() {
            return Err(OperationError::Rejected(
                "code-context project supply is external; reindex it through the deployment"
                    .to_string(),
            ));
        }
        if let ProjectState::Indexing { id, .. } = self.project_state() {
            return Err(OperationError::Rejected(format!(
                "code-context project {id} is already indexing; wait for it to finish"
            )));
        }
        // Reachability first: a retry against a dead service would only flip
        // the state to "indexing" and fail a moment later.
        self.endpoint_for(CallScope::Operations)
            .await
            .map_err(OperationError::Service)?;
        let project_id = self.operational_project_id().await?;
        self.set_project(ProjectState::Indexing {
            id: project_id,
            started_at_ms: now_ms(),
        });
        let service = Arc::clone(self);
        tokio::spawn(async move {
            service.run_full_index(project_id).await;
        });
        Ok(project_id)
    }

    /// Endpoint, client and project id for one operational call: the same
    /// gate as a query, then the project id that maintenance may act on.
    async fn operation_context(
        self: &Arc<Self>,
    ) -> Result<(String, reqwest::Client, i64), OperationError> {
        let endpoint = self
            .endpoint_for(CallScope::Operations)
            .await
            .map_err(OperationError::Service)?;
        let client = self.client().map_err(OperationError::Service)?;
        let project_id = self.operational_project_id().await?;
        Ok((endpoint, client, project_id))
    }

    /// Project id maintenance acts on.
    ///
    /// Unlike retrieval, an id is enough: a project that is still indexing or
    /// whose index failed is precisely what an operator needs to inspect. The
    /// resolution path is only entered when no id is known yet, so a fresh
    /// workspace still gets supplied lazily.
    async fn operational_project_id(self: &Arc<Self>) -> Result<i64, OperationError> {
        if self.supplies_project() {
            if let Some(id) = self.project_state().id() {
                return Ok(id);
            }
        }
        self.project_id_for(None)
            .await
            .map_err(OperationError::Rejected)
    }

    /// Ask the service to keep the index current. Managed supply owns the
    /// watch; the external mode never touches it.
    async fn start_watch(&self, project_id: i64) {
        let policy = self.project_policy();
        if policy.supply != IndexSupplyMode::Managed || !policy.managed_watch {
            return;
        }
        let Some(endpoint) = self.endpoint() else {
            return;
        };
        let Ok(client) = self.client() else {
            return;
        };
        if let Err(error) =
            start_watch(&client, &endpoint, project_id, policy.watch_timeout_ms).await
        {
            tracing::warn!(
                project_id,
                reason = %error.reason,
                "code-context index watch not started; the index will drift until the next full index"
            );
        }
    }

    async fn ensure_health(&self, endpoint: &str) -> Result<HealthReport, String> {
        if let Some(report) = self.fresh_report() {
            return Ok(report);
        }
        let _guard = self.probe.lock().await;
        if let Some(report) = self.fresh_report() {
            return Ok(report);
        }
        self.probe_endpoint(endpoint).await
    }

    async fn probe_endpoint(&self, endpoint: &str) -> Result<HealthReport, String> {
        let client = self.client()?;
        let report = match probe_health(&client, endpoint, HEALTH_PROBE_TIMEOUT_MS).await {
            Ok(report) => report,
            Err(error) => {
                if error.is_credential() {
                    self.health_write().credential_error = Some(error.reason.clone());
                }
                return Err(error.reason);
            }
        };
        let mut state = self.health_write();
        state.report = Some(report);
        state.probed_at = Some(Instant::now());
        Ok(report)
    }

    /// Run the capability handshake once. A missing or undecodable project
    /// endpoint means the deployed service speaks a different contract, so
    /// the mismatch is reported here instead of by every call.
    async fn ensure_capabilities(&self, endpoint: &str) -> Result<(), String> {
        if let Some(report) = self.capabilities_read().as_ref() {
            return if report.compatible {
                Ok(())
            } else {
                Err(incompatible_reason(endpoint, &report.missing))
            };
        }
        let _guard = self.capability_gate.lock().await;
        if let Some(report) = self.capabilities_read().as_ref() {
            return if report.compatible {
                Ok(())
            } else {
                Err(incompatible_reason(endpoint, &report.missing))
            };
        }
        let client = self.client()?;
        let timeout_ms = self.config().transport.timeout_ms;
        let mut missing = Vec::new();
        match list_projects(&client, endpoint, timeout_ms).await {
            Ok(_) => {}
            Err(error) if error.is_credential() => {
                self.health_write().credential_error = Some(error.reason.clone());
                return Err(error.reason);
            }
            Err(error) => missing.push(format!(
                "project endpoint {} ({})",
                super::project::PROJECTS_ENDPOINT,
                error.reason
            )),
        }
        let report = CapabilityReport {
            compatible: missing.is_empty(),
            missing,
            checked_at_ms: now_ms(),
        };
        let incompatible = !report.compatible;
        let missing = report.missing.clone();
        self.capabilities_write().replace(report);
        if incompatible {
            return Err(incompatible_reason(endpoint, &missing));
        }
        Ok(())
    }

    fn fresh_report(&self) -> Option<HealthReport> {
        let state = self.health_read();
        let report = state.report?;
        let probed_at = state.probed_at?;
        (probed_at.elapsed() < Duration::from_millis(HEALTH_MAX_AGE_MS)).then_some(report)
    }

    fn sidecar_snapshot(&self) -> Option<SidecarStatus> {
        let guard = self.lock_sidecar();
        let handle = guard.as_ref()?;
        let status = match handle.lock() {
            Ok(status) => status.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        Some(status)
    }

    fn set_project(&self, state: ProjectState) {
        match self.project.write() {
            Ok(mut guard) => *guard = state,
            Err(poisoned) => *poisoned.into_inner() = state,
        }
    }

    fn config_read(&self) -> std::sync::RwLockReadGuard<'_, CodeContextConfig> {
        match self.config.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn config_write(&self) -> std::sync::RwLockWriteGuard<'_, CodeContextConfig> {
        match self.config.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn endpoint_read(&self) -> std::sync::RwLockReadGuard<'_, Option<String>> {
        match self.endpoint.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn endpoint_write(&self) -> std::sync::RwLockWriteGuard<'_, Option<String>> {
        match self.endpoint.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn client_read(&self) -> std::sync::RwLockReadGuard<'_, Result<reqwest::Client, String>> {
        match self.client.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn client_write(&self) -> std::sync::RwLockWriteGuard<'_, Result<reqwest::Client, String>> {
        match self.client.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn project_read(&self) -> std::sync::RwLockReadGuard<'_, ProjectState> {
        match self.project.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn capabilities_read(&self) -> std::sync::RwLockReadGuard<'_, Option<CapabilityReport>> {
        match self.capabilities.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn capabilities_write(&self) -> std::sync::RwLockWriteGuard<'_, Option<CapabilityReport>> {
        match self.capabilities.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn health_read(&self) -> std::sync::RwLockReadGuard<'_, HealthState> {
        match self.health.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn health_write(&self) -> std::sync::RwLockWriteGuard<'_, HealthState> {
        match self.health.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_sidecar(&self) -> std::sync::MutexGuard<'_, Option<SidecarStatusHandle>> {
        match self.sidecar.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl Default for CodeContextService {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Message for a cached incompatible verdict: the same wording as the first
/// failure, so the state reads identically however often it is observed.
fn incompatible_reason(endpoint: &str, missing: &[String]) -> String {
    format!(
        "code-context service at {endpoint} is incompatible: missing {}",
        missing.join(", ")
    )
}

/// Whether the managed process must be replaced because its launch line or
/// address changed. Policy-only changes never require a restart.
fn managed_restart_reason(
    previous: &CodeContextConfig,
    next: &CodeContextConfig,
) -> Option<String> {
    if !next.enabled || next.transport.transport_mode != TransportMode::Managed {
        return None;
    }
    if previous.transport.transport_mode != TransportMode::Managed {
        return Some("transport mode changed to managed".to_string());
    }
    if previous.transport.managed_binary != next.transport.managed_binary {
        return Some("managed binary changed".to_string());
    }
    if previous.transport.managed_port != next.transport.managed_port {
        return Some("managed port changed".to_string());
    }
    if previous.transport.auth_token_env != next.transport.auth_token_env {
        return Some("managed credential changed".to_string());
    }
    None
}

/// Shared service handle used by every consumer.
pub type SharedCodeContextService = Arc<CodeContextService>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ServiceTransport;

    fn external_config(base_url: &str) -> CodeContextConfig {
        CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some(base_url.to_string()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn disabled_service_rejects_every_scope() {
        let service = CodeContextService::disabled();
        let error = service
            .endpoint_for(CallScope::Fold)
            .await
            .expect_err("disabled service must reject");
        assert!(error.contains("disabled"), "{error}");
        assert!(!service.is_enabled());
        assert!(service.endpoint().is_none());
        assert_eq!(service.availability(), CodeContextAvailability::none());
    }

    #[tokio::test]
    async fn enabled_service_without_endpoint_reports_unresolved() {
        let service = CodeContextService::external(CodeContextConfig {
            enabled: true,
            ..Default::default()
        });
        let error = service
            .endpoint_for(CallScope::Fold)
            .await
            .expect_err("unresolved endpoint must reject");
        assert!(error.contains("unresolved"), "{error}");
    }

    #[tokio::test]
    async fn capability_gate_follows_the_health_verdict() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let server = tokio::spawn(async move { serve_degraded_service(listener).await });
        let service =
            CodeContextService::external(external_config(&format!("http://127.0.0.1:{port}")));

        assert_eq!(
            service.endpoint_for(CallScope::Fold).await.expect("fold"),
            format!("http://127.0.0.1:{port}")
        );
        let error = service
            .endpoint_for(CallScope::HybridSearch)
            .await
            .expect_err("missing embedding must reject hybrid search");
        assert!(error.contains("embedding service"), "{error}");
        assert_eq!(
            service.endpoint_for(CallScope::KeywordSearch).await,
            Ok(format!("http://127.0.0.1:{port}"))
        );

        let status = service.status();
        let health = status.health.expect("health cached");
        assert!(!health.embedding);
        assert!(health.bm25);
        assert!(status.capabilities.expect("capabilities").compatible);
        assert!(!status.availability.semantic);
        assert!(status.availability.keyword);
        server.abort();
    }

    #[tokio::test]
    async fn startup_error_short_circuits_calls() {
        let service = CodeContextService::managed(
            CodeContextConfig {
                enabled: true,
                transport: ServiceTransport {
                    transport_mode: TransportMode::Managed,
                    ..Default::default()
                },
                ..Default::default()
            },
            "http://127.0.0.1:1".to_string(),
        );
        service.record_startup_error("binary not found");
        let error = service
            .endpoint_for(CallScope::Fold)
            .await
            .expect_err("startup failure must reject");
        assert!(error.contains("binary not found"), "{error}");
        assert_eq!(service.availability(), CodeContextAvailability::none());
    }

    #[test]
    fn missing_credential_blocks_every_scope_without_echoing_it() {
        let service = CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9000".into()),
                auth_token_env: Some("WF_CCE_TEST_MISSING_TOKEN".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        let status = service.status();
        let error = status.credential_error.expect("credential error");
        assert!(error.contains("WF_CCE_TEST_MISSING_TOKEN"), "{error}");
        assert_eq!(service.availability(), CodeContextAvailability::none());
    }

    #[tokio::test]
    async fn incompatible_service_fails_the_handshake_once() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let project_probes = Arc::new(Mutex::new(0u32));
        let counter = Arc::clone(&project_probes);
        let server = tokio::spawn(async move { serve_health_only(listener, counter).await });
        let service =
            CodeContextService::external(external_config(&format!("http://127.0.0.1:{port}")));
        let error = service
            .endpoint_for(CallScope::Fold)
            .await
            .expect_err("missing project endpoint must reject");
        assert!(error.contains("incompatible"), "{error}");
        let capabilities = service.status().capabilities.expect("capabilities cached");
        assert!(!capabilities.compatible);
        assert_eq!(service.availability(), CodeContextAvailability::none());

        // The verdict is cached: later calls fail the same way without
        // re-probing the missing endpoint.
        let second = service
            .endpoint_for(CallScope::Fold)
            .await
            .expect_err("incompatible service stays rejected");
        assert!(second.contains("incompatible"), "{second}");
        assert_eq!(*project_probes.lock().expect("probe count"), 1);
        server.abort();
    }

    #[tokio::test]
    async fn external_supply_reads_the_configured_default_project() {
        let service = Arc::new(CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                ..Default::default()
            },
            retrieval: crate::RetrievalPolicy {
                default_project_id: Some(4),
                ..Default::default()
            },
            project: ProjectPolicy {
                supply: IndexSupplyMode::External,
                ..Default::default()
            },
            ..Default::default()
        }));
        assert_eq!(service.project_id_for(None).await, Ok(4));
        assert_eq!(service.project_id_for(Some(11)).await, Ok(11));
        assert_eq!(service.project_state(), ProjectState::NotSupplied);

        let unset = Arc::new(CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                ..Default::default()
            },
            project: ProjectPolicy {
                supply: IndexSupplyMode::External,
                ..Default::default()
            },
            ..Default::default()
        }));
        let error = unset
            .project_id_for(None)
            .await
            .expect_err("no project must be a project-state problem");
        assert!(error.contains("default_project_id"), "{error}");
    }

    #[test]
    fn managed_supply_without_workspace_reports_unresolved() {
        let service = Arc::new(CodeContextService::managed(
            CodeContextConfig {
                enabled: true,
                transport: ServiceTransport {
                    transport_mode: TransportMode::Managed,
                    ..Default::default()
                },
                project: ProjectPolicy {
                    supply: IndexSupplyMode::Managed,
                    ..Default::default()
                },
                ..Default::default()
            },
            "http://127.0.0.1:9".to_string(),
        ));
        assert!(matches!(
            service.project_state(),
            ProjectState::Unresolved { .. }
        ));
        assert!(service.supplies_project());
    }

    #[tokio::test]
    async fn apply_config_resets_verdicts_and_reports_managed_restart() {
        let service = CodeContextService::external(external_config("http://127.0.0.1:9"));
        let outcome = service
            .apply_config(CodeContextConfig {
                enabled: true,
                transport: ServiceTransport {
                    transport_mode: TransportMode::Managed,
                    managed_port: 9123,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await;
        assert!(outcome.restart_managed);
        assert!(outcome.reason.unwrap().contains("managed"), "reason");
        assert!(service.status().health.is_none());
    }

    /// Scripted project API: records every request line and answers the
    /// project/index/watch surfaces the supply path uses.
    struct ScriptedService {
        created: std::sync::Mutex<bool>,
    }

    impl ScriptedService {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                created: std::sync::Mutex::new(false),
            })
        }

        fn created(&self) -> bool {
            *self.created.lock().expect("created lock")
        }

        fn mark_created(&self) {
            *self.created.lock().expect("created lock") = true;
        }
    }

    fn external_supply_config(base_url: &str) -> CodeContextConfig {
        CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some(base_url.to_string()),
                timeout_ms: 5_000,
                ..Default::default()
            },
            project: ProjectPolicy {
                supply: IndexSupplyMode::Managed,
                managed_watch: true,
                watch_timeout_ms: 2_000,
            },
            ..Default::default()
        }
    }

    struct SupplyFixture {
        service: Arc<CodeContextService>,
        requests: Arc<Mutex<Vec<String>>>,
        root: PathBuf,
        server: tokio::task::JoinHandle<()>,
    }

    async fn supply_fixture(index_delay_ms: u64) -> SupplyFixture {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let state = ScriptedService::new();
        let root = std::env::temp_dir().join(format!(
            "wf-cce-supply-{}-{}",
            std::process::id(),
            index_delay_ms
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("create workspace");
        let server = tokio::spawn(serve_project_api(
            listener,
            Arc::clone(&requests),
            state,
            index_delay_ms,
            root.to_string_lossy().into_owned(),
        ));
        let service = Arc::new(
            CodeContextService::external(external_supply_config(&format!(
                "http://127.0.0.1:{port}"
            )))
            .with_workspace_root(Some(root.clone())),
        );
        SupplyFixture {
            service,
            requests,
            root,
            server,
        }
    }

    fn recorded(requests: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        requests.lock().expect("requests lock").clone()
    }

    #[tokio::test]
    async fn supply_creates_the_project_and_indexes_it_once() {
        let fixture = supply_fixture(0).await;
        fixture
            .service
            .supply()
            .await
            .expect("supply resolves the project");
        let mut last_indexed = None;
        for _ in 0..100 {
            if let ProjectState::Ready {
                id,
                last_indexed: indexed,
            } = fixture.service.project_state()
            {
                assert_eq!(id, 5);
                last_indexed = indexed;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(
            last_indexed.as_deref(),
            Some("2024-05-01T00:00:00Z"),
            "state: {:?}",
            fixture.service.project_state()
        );

        let requests = recorded(&fixture.requests);
        assert!(
            requests.contains(&"GET /api/project".to_string()),
            "{requests:?}"
        );
        assert!(
            requests.contains(&"POST /api/project".to_string()),
            "{requests:?}"
        );
        assert!(
            requests.contains(&"POST /api/project/5/index".to_string()),
            "{requests:?}"
        );
        assert!(
            requests
                .iter()
                .any(|request| request.ends_with("/watch/start")),
            "{requests:?}"
        );
        let index_calls = requests
            .iter()
            .filter(|request| request.ends_with("/index"))
            .count();
        assert_eq!(index_calls, 1);

        // A second supply pass must not index again.
        let before = recorded(&fixture.requests).len();
        fixture.service.supply().await.expect("second supply");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after = recorded(&fixture.requests);
        assert!(
            !after[before.min(after.len())..]
                .iter()
                .any(|request| request.ends_with("/index")),
            "{after:?}"
        );
        assert_eq!(
            fixture.service.project_id_for(None).await,
            Ok(5),
            "a ready project is readable without further resolution"
        );

        fixture.server.abort();
        std::fs::remove_dir_all(&fixture.root).ok();
    }

    #[tokio::test]
    async fn indexing_project_reports_progress_and_recovers() {
        let fixture = supply_fixture(600).await;
        fixture.service.supply().await.expect("supply resolves");
        assert!(
            matches!(
                fixture.service.project_state(),
                ProjectState::Indexing { id: 5, .. }
            ),
            "state: {:?}",
            fixture.service.project_state()
        );
        let error = fixture
            .service
            .project_id_for(None)
            .await
            .expect_err("retrieval must refuse during the first index");
        assert!(error.contains("indexing"), "{error}");

        for _ in 0..100 {
            if matches!(fixture.service.project_state(), ProjectState::Ready { .. }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(fixture.service.project_id_for(None).await, Ok(5));

        fixture.server.abort();
        std::fs::remove_dir_all(&fixture.root).ok();
    }

    #[tokio::test]
    async fn unreachable_service_keeps_the_project_unresolved() {
        let root = std::env::temp_dir().join(format!("wf-cce-unreachable-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create workspace");
        // A freshly released port: nothing listens on it, and its origin has
        // no shared breaker history with other tests.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("bind port");
        let port = probe.local_addr().expect("port").port();
        drop(probe);
        let base_url = format!("http://127.0.0.1:{port}");
        let service = Arc::new(
            CodeContextService::external(external_supply_config(&base_url))
                .with_workspace_root(Some(root.clone())),
        );
        let error = service.supply().await.expect_err("supply must fail");
        assert!(error.contains(&base_url), "{error}");
        assert!(matches!(
            service.project_state(),
            ProjectState::Unresolved { .. }
        ));
        // The next consumer retries resolution instead of needing a restart.
        let error = service
            .project_id_for(None)
            .await
            .expect_err("unresolved project must reject retrieval");
        assert!(
            error.contains(&base_url) || error.contains("circuit open"),
            "{error}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    async fn serve_project_api(
        listener: tokio::net::TcpListener,
        requests: Arc<Mutex<Vec<String>>>,
        state: Arc<ScriptedService>,
        index_delay_ms: u64,
        root_path: String,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 2048];
            loop {
                match socket.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
                if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&buffer).to_string();
            let request_line = text.lines().next().unwrap_or_default().to_string();
            let mut parts = request_line.split(' ');
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or("/").to_string();
            requests
                .lock()
                .expect("requests lock")
                .push(format!("{method} {path}"));

            let (status, body) = match (method.as_str(), path.as_str()) {
                ("GET", "/api/project") => {
                    if state.created() {
                        let body = format!(
                            r#"{{"projects":[{{"id":"5","root_path":{},"last_indexed":"2024-05-01T00:00:00Z"}}]}}"#,
                            serde_json::json!(root_path)
                        );
                        ("200 OK", body)
                    } else {
                        ("200 OK", r#"{"projects":[]}"#.to_string())
                    }
                }
                ("POST", "/api/project") => {
                    state.mark_created();
                    let body = format!(
                        r#"{{"project":{{"id":"5","root_path":{},"last_indexed":null}}}}"#,
                        serde_json::json!(root_path)
                    );
                    ("200 OK", body)
                }
                ("POST", _) if path.ends_with("/index") => {
                    if index_delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(index_delay_ms)).await;
                    }
                    ("200 OK", r#"{"success":true}"#.to_string())
                }
                ("POST", _) if path.ends_with("/watch/start") => {
                    ("200 OK", r#"{"success":true}"#.to_string())
                }
                _ => ("404 NOT FOUND", "{}".to_string()),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }

    /// Scripted maintenance service: health, project supply, a failing index
    /// budget and the three dead-letter surfaces. `fail_index_calls` leading
    /// index calls answer 500 so the failed-project path is real.
    async fn maintenance_fixture(fail_index_calls: u32, index_delay_ms: u64) -> SupplyFixture {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let state = ScriptedService::new();
        let root = std::env::temp_dir().join(format!(
            "wf-cce-maintenance-{}-{}",
            std::process::id(),
            fail_index_calls
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("create workspace");
        let server = tokio::spawn(serve_maintenance_api(
            listener,
            Arc::clone(&requests),
            state,
            fail_index_calls,
            index_delay_ms,
            root.to_string_lossy().into_owned(),
        ));
        let service = Arc::new(
            CodeContextService::external(external_supply_config(&format!(
                "http://127.0.0.1:{port}"
            )))
            .with_workspace_root(Some(root.clone())),
        );
        SupplyFixture {
            service,
            requests,
            root,
            server,
        }
    }

    async fn serve_maintenance_api(
        listener: tokio::net::TcpListener,
        requests: Arc<Mutex<Vec<String>>>,
        state: Arc<ScriptedService>,
        fail_index_calls: u32,
        index_delay_ms: u64,
        root_path: String,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut failures_left = fail_index_calls;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 2048];
            loop {
                match socket.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
                if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&buffer).to_string();
            let request_line = text.lines().next().unwrap_or_default().to_string();
            let mut parts = request_line.split(' ');
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or("/").to_string();
            requests
                .lock()
                .expect("requests lock")
                .push(format!("{method} {path}"));

            let index_body = if index_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(index_delay_ms)).await;
                r#"{"files":[]}"#
            } else {
                r#"{"project_id":5,"files":[{"file_path":"src/big.rs","version":2,"updated_at":"2024-05-01T00:00:00Z","modules":[{"module":"embedding","retry_count":3,"truncated":false,"acknowledged":false}]}]}"#
            };
            let (status, body) = match (method.as_str(), path.as_str()) {
                ("GET", "/api/health") => ("200 OK", r#"{"healthy":true}"#.to_string()),
                ("GET", "/api/project") => {
                    if state.created() {
                        let body = format!(
                            r#"{{"projects":[{{"id":"5","root_path":{},"last_indexed":"2024-05-01T00:00:00Z"}}]}}"#,
                            serde_json::json!(root_path)
                        );
                        ("200 OK", body)
                    } else {
                        ("200 OK", r#"{"projects":[]}"#.to_string())
                    }
                }
                ("POST", "/api/project") => {
                    state.mark_created();
                    let body = format!(
                        r#"{{"project":{{"id":"5","root_path":{},"last_indexed":null}}}}"#,
                        serde_json::json!(root_path)
                    );
                    ("200 OK", body)
                }
                ("POST", _) if path.ends_with("/index") => {
                    if failures_left > 0 {
                        failures_left -= 1;
                        (
                            "500 INTERNAL SERVER ERROR",
                            r#"{"error":"embedding down"}"#.to_string(),
                        )
                    } else {
                        ("200 OK", r#"{"success":true}"#.to_string())
                    }
                }
                ("POST", _) if path.ends_with("/watch/start") => {
                    ("200 OK", r#"{"success":true}"#.to_string())
                }
                ("GET", _) if path.ends_with("/dead-letters") => ("200 OK", index_body.to_string()),
                ("POST", _) if path.ends_with("/dead-letters/retry") => (
                    "200 OK",
                    r#"{"success":true,"affected":1,"message":"retried 1 file"}"#.to_string(),
                ),
                ("POST", _) if path.ends_with("/dead-letters/acknowledge") => (
                    "200 OK",
                    r#"{"success":true,"affected":1,"message":"acknowledged"}"#.to_string(),
                ),
                _ => ("404 NOT FOUND", "{}".to_string()),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }

    async fn wait_for(
        service: &Arc<CodeContextService>,
        predicate: impl Fn(&ProjectState) -> bool,
    ) {
        for _ in 0..200 {
            if predicate(&service.project_state()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("state never matched, last: {:?}", service.project_state());
    }

    #[tokio::test]
    async fn maintenance_works_on_a_failed_project_and_reindex_recovers_it() {
        let fixture = maintenance_fixture(1, 0).await;
        fixture.service.supply().await.expect("supply resolves");
        wait_for(&fixture.service, |state| {
            matches!(state, ProjectState::Failed { id: Some(5), .. })
        })
        .await;

        // Retrieval refuses, maintenance does not: that is the whole point of
        // keeping the project id across the failure.
        let error = fixture
            .service
            .project_id_for(None)
            .await
            .expect_err("a failed index blocks retrieval");
        assert!(error.contains("unavailable"), "{error}");
        let list = fixture
            .service
            .dead_letters()
            .await
            .expect("dead letters stay readable");
        assert_eq!(list.project_id, 5);
        assert_eq!(list.files[0].file_path, "src/big.rs");
        assert_eq!(list.pending_modules(), 1);

        let retried = fixture
            .service
            .retry_dead_letters(vec!["src/big.rs".to_string()])
            .await
            .expect("retry accepted");
        assert!(retried.success);
        let acknowledged = fixture
            .service
            .acknowledge_dead_letter("src/big.rs".to_string(), Some("embedding".to_string()))
            .await
            .expect("acknowledge accepted");
        assert_eq!(acknowledged.affected, 1);

        let project_id = fixture
            .service
            .retry_index()
            .await
            .expect("reindex accepted");
        assert_eq!(project_id, 5);
        wait_for(&fixture.service, |state| {
            matches!(state, ProjectState::Ready { id: 5, .. })
        })
        .await;

        let requests = recorded(&fixture.requests);
        assert!(
            requests.contains(&"GET /api/project/5/dead-letters".to_string()),
            "{requests:?}"
        );
        assert!(
            requests.contains(&"POST /api/project/5/dead-letters/retry".to_string()),
            "{requests:?}"
        );
        assert!(
            requests.contains(&"POST /api/project/5/dead-letters/acknowledge".to_string()),
            "{requests:?}"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.ends_with("/index"))
                .count(),
            2,
            "the manual reindex is the second index run: {requests:?}"
        );

        fixture.server.abort();
        std::fs::remove_dir_all(&fixture.root).ok();
    }

    #[tokio::test]
    async fn reindex_is_rejected_while_indexing_or_without_runtime_supply() {
        let fixture = maintenance_fixture(0, 400).await;
        fixture.service.supply().await.expect("supply resolves");
        let error = fixture
            .service
            .retry_index()
            .await
            .expect_err("a running index must not be restarted");
        assert!(matches!(error, OperationError::Rejected(_)), "{error}");
        assert!(error.to_string().contains("already indexing"), "{error}");

        let external = Arc::new(CodeContextService::external(external_config(
            "http://127.0.0.1:9",
        )));
        let error = external
            .retry_index()
            .await
            .expect_err("external supply owns its own indexes");
        assert!(matches!(error, OperationError::Rejected(_)), "{error}");
        assert!(error.to_string().contains("external"), "{error}");

        fixture.server.abort();
        std::fs::remove_dir_all(&fixture.root).ok();
    }

    async fn serve_degraded_service(listener: tokio::net::TcpListener) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0u8; 2048];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let body = if request.contains("/api/project") {
                r#"{"projects":[]}"#.to_string()
            } else {
                r#"{"healthy":false,"qdrant":{"reachable":true},"bm25":{"reachable":true},"embedding":{"reachable":false}}"#.to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }

    async fn serve_health_only(listener: tokio::net::TcpListener, project_probes: Arc<Mutex<u32>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0u8; 2048];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            if request.contains("/api/project") {
                *project_probes.lock().expect("probe count") += 1;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 404 NOT FOUND\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    )
                    .await;
                continue;
            }
            let body = r#"{"healthy":true,"qdrant":{"reachable":true},"bm25":{"reachable":true},"embedding":{"reachable":true}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }
}
