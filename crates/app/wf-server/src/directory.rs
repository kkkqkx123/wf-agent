//! Request-scoped directory resolution for multi-directory serving.
//!
//! A single server process serves many business directories: every
//! workspace-bound request carries exactly one directory, and the resolved
//! scope (canonical directory, owning project, stable project id) travels
//! in request extensions for domain handlers to anchor on. Transport is
//! dual-channel: the `x-wf-directory` header wins for regular calls, the
//! `directory` query parameter covers channels that cannot set headers
//! (event streams, websocket upgrades).
//!
//! There is no implicit server-side default workspace: a directory-less
//! request is never silently bound to the server's own working directory.
//! Workspace-bound handlers reject it and global handlers ignore it,
//! unless the operator explicitly configured a single-machine default
//! (`DirectoryContext::with_default`, `--directory`). Silence here would
//! mean operating in a directory the client never named, so absence
//! fails loud at the handler instead of falling back.
//!
//! Directory identity is deterministic (canonical path plus VCS-root project
//! discovery with a stable hash), so no persisted project table is needed
//! for routing. The per-directory entry cache only memoizes validated
//! scopes; it never invents them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use wf_common::workspace::{discover_project_root, project_id_for};
use wf_config::env::{env_parse_list, EnvValue};

/// Request header carrying the business directory. Wins over the query
/// parameter when both are present.
pub const DIRECTORY_HEADER: &str = "x-wf-directory";

/// Query parameter carrying the business directory for channels that cannot
/// set headers (server-sent events, websocket upgrades).
pub const DIRECTORY_QUERY_PARAM: &str = "directory";

/// Environment variable pinning the allowed directory roots
/// (comma-separated absolute paths). Wins over the file layer; empty means
/// only the default project root is served.
pub const DIRECTORY_ROOTS_ENV_VAR: &str = "WF_DIRECTORY_ROOTS";

/// Idle time after which a cached directory entry is evicted on the next
/// resolve. Live executions never depend on the cache, so eviction only
/// costs one revalidation.
const ENTRY_IDLE_TTL: Duration = Duration::from_secs(3600);

/// Sweep the entry cache once it grows past this many entries. Keeps the
/// per-request resolve cost flat for the common few-directories case.
const ENTRY_SWEEP_THRESHOLD: usize = 16;

/// Validated per-request directory scope shared with domain handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryScope {
    /// Canonical business directory the request is anchored on.
    pub directory: PathBuf,
    /// Owning project root (nearest VCS root above the directory, or the
    /// directory itself when version control is absent).
    pub project: PathBuf,
    /// Stable id derived from the canonical project path.
    pub project_id: String,
}

/// Directory allowlist configuration. Empty permits every validated
/// directory; a non-empty list restricts serving to the listed roots.
#[derive(Debug, Clone, Default)]
pub struct DirectoryConfig {
    pub allowed_roots: Vec<PathBuf>,
}

impl DirectoryConfig {
    /// Resolve the effective allowlist: a non-empty environment value wins
    /// over the file layer, otherwise the file value applies.
    pub fn resolve(file_roots: Option<Vec<String>>) -> Self {
        if let Ok(raw) = std::env::var(DIRECTORY_ROOTS_ENV_VAR) {
            if !raw.trim().is_empty() {
                let mut roots = Vec::new();
                match env_parse_list(&raw) {
                    Ok(EnvValue::List(list)) => {
                        for item in list {
                            if !item.is_empty() {
                                roots.push(PathBuf::from(item));
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, var = DIRECTORY_ROOTS_ENV_VAR, "invalid directory roots; keeping file value");
                    }
                }
                if !roots.is_empty() {
                    return Self {
                        allowed_roots: roots,
                    };
                }
            }
        }
        Self {
            allowed_roots: file_roots
                .unwrap_or_default()
                .into_iter()
                .filter(|root| !root.trim().is_empty())
                .map(PathBuf::from)
                .collect(),
        }
    }

    /// Whether the canonical directory may be served. An empty allowlist
    /// permits everything validated: the client named the directory
    /// explicitly, so nothing is silent.
    fn allows(&self, directory: &Path) -> bool {
        if self.allowed_roots.is_empty() {
            return true;
        }
        self.allowed_roots.iter().any(|root| {
            let anchor = canonicalize_if_exists(root);
            directory == anchor || directory.starts_with(&anchor)
        })
    }
}

/// Typed directory rejection. Converts to the shared API error taxonomy so
/// handlers and middleware render through one envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryError {
    Empty,
    NotAbsolute(String),
    NotFound(String),
    NotDirectory(String),
    Forbidden(String),
}

impl std::fmt::Display for DirectoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "directory must not be empty"),
            Self::NotAbsolute(raw) => write!(f, "directory must be absolute: {raw}"),
            Self::NotFound(raw) => write!(f, "directory does not exist: {raw}"),
            Self::NotDirectory(raw) => write!(f, "directory is not a directory: {raw}"),
            Self::Forbidden(raw) => write!(f, "directory is not served: {raw}"),
        }
    }
}

impl std::error::Error for DirectoryError {}

impl From<DirectoryError> for wf_api::ApiError {
    fn from(err: DirectoryError) -> Self {
        match err {
            DirectoryError::NotFound(raw) => wf_api::ApiError::NotFound {
                entity_type: "directory".to_string(),
                id: raw,
            },
            other => wf_api::ApiError::Validation(other.to_string()),
        }
    }
}

/// Decode one query-string value: `+` becomes a space, `%XX` becomes the
/// byte. Malformed escapes pass through verbatim so a bad value fails
/// validation instead of failing decoding.
fn decode_query_value(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() => {
                let hex = &raw[i + 1..i + 3.min(raw.len() - i)];
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(hex, 16) {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
                out.push(b'%');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extract the raw request directory: header wins, query parameter covers
/// headerless channels. Returns `None` when the request carries neither.
pub fn extract_raw(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(value) = headers
        .get(DIRECTORY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_string());
    }
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some(split) => split,
            None => continue,
        };
        if key == DIRECTORY_QUERY_PARAM {
            let decoded = decode_query_value(value);
            if !decoded.trim().is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

/// Best-effort canonicalization for allowlist anchors: existing paths
/// resolve symlinks so equivalent spellings compare equal.
fn canonicalize_if_exists(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Validate and canonicalize one raw directory value. Existence and
/// directory checks happen before symlink resolution so every rejection
/// carries the user-supplied spelling.
pub fn canonicalize_dir(raw: &str) -> Result<PathBuf, DirectoryError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DirectoryError::Empty);
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(DirectoryError::NotAbsolute(trimmed.to_string()));
    }
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() && !metadata.is_dir() {
                return Err(DirectoryError::NotDirectory(trimmed.to_string()));
            }
        }
        Err(_) => return Err(DirectoryError::NotFound(trimmed.to_string())),
    }
    std::fs::canonicalize(&path).map_err(|_| DirectoryError::NotFound(trimmed.to_string()))
}

/// Resolve one raw directory value (or its absence) to a validated scope.
/// `None` input yields `None`: there is no default workspace, so a
/// directory-less request carries no scope and workspace-bound handlers
/// reject it instead of guessing.
pub fn resolve_scope(
    raw: Option<&str>,
    config: &DirectoryConfig,
) -> Result<Option<DirectoryScope>, DirectoryError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let directory = canonicalize_dir(raw)?;
    if !config.allows(&directory) {
        return Err(DirectoryError::Forbidden(
            directory.to_string_lossy().into_owned(),
        ));
    }
    let project = discover_project_root(&directory);
    let project_id = project_id_for(&project);
    Ok(Some(DirectoryScope {
        directory,
        project,
        project_id,
    }))
}

#[derive(Debug, Clone)]
struct CachedEntry {
    scope: DirectoryScope,
    last_seen: Instant,
}

/// Shared directory resolution state: allowlist plus the validated-scope
/// cache. The cache memoizes validation only; every entry revalidates
/// on idle expiry through the normal resolve path.
#[derive(Debug)]
pub struct DirectoryContext {
    config: DirectoryConfig,
    entries: dashmap::DashMap<String, CachedEntry>,
    ready: AtomicBool,
    /// Single-machine default workspace: directory-less requests resolve
    /// to this prevalidated scope instead of failing. `None` keeps the
    /// fail-loud behavior for multi-tenant serving.
    default: Option<DirectoryScope>,
}

impl DirectoryContext {
    pub fn new(config: DirectoryConfig) -> Self {
        Self {
            config,
            entries: dashmap::DashMap::new(),
            ready: AtomicBool::new(true),
            default: None,
        }
    }

    /// Build a context with a single-machine default workspace. The
    /// directory is validated once here (existence, directory shape,
    /// allowlist) with project discovery precomputed, so startup fails
    /// fast on a typo instead of failing every directory-less request.
    pub fn with_default(config: DirectoryConfig, directory: &Path) -> Result<Self, DirectoryError> {
        let canonical = canonicalize_dir(directory.to_string_lossy().as_ref())?;
        if !config.allows(&canonical) {
            return Err(DirectoryError::Forbidden(
                canonical.to_string_lossy().into_owned(),
            ));
        }
        let project = discover_project_root(&canonical);
        let project_id = project_id_for(&project);
        Ok(Self {
            config,
            entries: dashmap::DashMap::new(),
            ready: AtomicBool::new(true),
            default: Some(DirectoryScope {
                directory: canonical,
                project,
                project_id,
            }),
        })
    }

    /// The configured default workspace, if any.
    pub fn default_scope(&self) -> Option<&DirectoryScope> {
        self.default.as_ref()
    }

    /// Flip the serving gate. Embedders mark the context not ready while
    /// plugins activate; requests during the window fail fast with 503.
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    /// Whether directory services are ready to serve requests.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Resolve the request directory from headers and query string. The
    /// canonical path is the cache key: hits skip project rediscovery and
    /// only refresh the idle clock, misses validate fully and populate the
    /// entry. Idle entries are swept once the cache grows past its
    /// threshold; every entry revalidates from disk after eviction.
    /// A directory-less request resolves to the configured default
    /// workspace when one exists, and to no scope otherwise.
    pub fn resolve(
        &self,
        headers: &HeaderMap,
        query: Option<&str>,
    ) -> Result<Option<DirectoryScope>, DirectoryError> {
        let raw = extract_raw(headers, query);
        let Some(raw) = raw else {
            return Ok(self.default.clone());
        };
        let directory = canonicalize_dir(&raw)?;
        if !self.config.allows(&directory) {
            return Err(DirectoryError::Forbidden(
                directory.to_string_lossy().into_owned(),
            ));
        }
        let key = directory.to_string_lossy().into_owned();
        if let Some(hit) = self.entries.get(&key).map(|entry| entry.scope.clone()) {
            self.entries.insert(
                key,
                CachedEntry {
                    scope: hit.clone(),
                    last_seen: Instant::now(),
                },
            );
            self.sweep_idle();
            return Ok(Some(hit));
        }
        let project = discover_project_root(&directory);
        let project_id = project_id_for(&project);
        let scope = DirectoryScope {
            directory,
            project,
            project_id,
        };
        self.entries.insert(
            key,
            CachedEntry {
                scope: scope.clone(),
                last_seen: Instant::now(),
            },
        );
        self.sweep_idle();
        Ok(Some(scope))
    }

    /// Drop one cached directory entry. The input is canonicalized first
    /// so equivalent spellings hit the same cache key. In-flight requests
    /// already holding the scope are unaffected; the next request
    /// revalidates from disk.
    pub fn invalidate(&self, directory: &str) -> bool {
        let key = canonicalize_dir(directory)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| directory.trim().to_string());
        if self.entries.remove(&key).is_some() {
            return true;
        }
        self.entries.remove(directory.trim()).is_some()
    }

    /// Canonical cache keys backing the location service map, for the
    /// debug surface. Read-only; keys are canonical directory paths.
    pub fn cached_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .entries
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        keys.sort();
        keys
    }

    /// Snapshot of every cached directory scope for the introspection
    /// surface: directory plus the owning project root and stable project
    /// id. Read-only; sorted by directory so repeated reads diff cleanly.
    pub fn cached_scopes(&self) -> Vec<DirectoryScope> {
        let mut scopes: Vec<DirectoryScope> = self
            .entries
            .iter()
            .map(|entry| entry.value().scope.clone())
            .collect();
        scopes.sort_by(|a, b| a.directory.cmp(&b.directory));
        scopes
    }

    /// Directory allowlist backing explicit request directories.
    pub fn config(&self) -> &DirectoryConfig {
        &self.config
    }

    /// Number of cached directory entries, for the system surface.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    fn sweep_idle(&self) {
        if self.entries.len() <= ENTRY_SWEEP_THRESHOLD {
            return;
        }
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.duration_since(entry.last_seen) < ENTRY_IDLE_TTL);
    }
}

/// Request middleware resolving the directory scope into extensions.
/// Directory rejections use the location-specific code so clients can
/// tell a wrong directory from a wrong entity id; every other request
/// continues with `Option<DirectoryScope>` present (possibly `None`).
/// Whether a target path stays inside the request scope. Canonicalizes
/// best-effort so equivalent spellings compare equal; unresolvable paths
/// fall back to lexical prefix comparison and fail closed.
pub(crate) fn ensure_path_owned(
    scope: &DirectoryScope,
    target: &Path,
) -> Result<(), DirectoryError> {
    let anchor =
        std::fs::canonicalize(&scope.directory).unwrap_or_else(|_| scope.directory.clone());
    let owned = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if owned == anchor || owned.starts_with(&anchor) {
        return Ok(());
    }
    Err(DirectoryError::Forbidden(
        owned.to_string_lossy().into_owned(),
    ))
}

pub(crate) async fn directory_middleware(
    State(ctx): State<Arc<DirectoryContext>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !ctx.is_ready() {
        return crate::envelope::service_unavailable("directory services are warming up");
    }
    let query = req.uri().query().map(str::to_string);
    let scope = match ctx.resolve(req.headers(), query.as_deref()) {
        Ok(scope) => scope,
        Err(crate::directory::DirectoryError::NotFound(raw)) => {
            return crate::envelope::location_not_found(format!("location not found: {raw}"));
        }
        Err(crate::directory::DirectoryError::Forbidden(raw)) => {
            // Allowed-root rejections stay distinct from missing locations:
            // 404 means the path is wrong, this branch means the server is
            // not configured to serve the path at all.
            return crate::envelope::forbidden(format!("location not served: {raw}"));
        }
        Err(err) => return crate::envelope::error_response(err.into()).into_response(),
    };
    let mut req = req;
    req.extensions_mut().insert(scope);
    next.run(req).await
}

/// Existence gate for execution-scoped read views (`GET
/// /api/v1/executions/{id}/*`). Control endpoints already fail unknown
/// ids as not-found through their owner gates, but the state / analysis /
/// graph read views degrade to empty results, so callers cannot tell an
/// empty execution from a missing one. This layer resolves the id across
/// workflow and agent-loop records (live plus persisted plus checkpoint
/// ghosts, mirroring `resolve_execution`) before any read handler runs:
/// unknown ids fail as not-found, existing ids pass through untouched.
///
/// Only `GET` sub-views are gated: detail and control routes keep their
/// own semantics, `POST` commands stay with their owner gates, and
/// idempotent `DELETE` clears keep succeeding on missing ids. Non-execution
/// prefixes (`/agent-loops/*`, `/unified-executions/*`, …) never match.
pub(crate) async fn execution_existence_middleware(
    State(ctx): State<Arc<wf_api::ApiContext>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.method() != axum::http::Method::GET {
        return next.run(req).await;
    }
    let Some(rest) = req.uri().path().strip_prefix("/api/v1/executions/") else {
        return next.run(req).await;
    };
    let mut segments = rest.split('/');
    let (Some(id), Some(_view)) = (segments.next(), segments.next()) else {
        return next.run(req).await;
    };
    if id.is_empty() {
        return next.run(req).await;
    }
    match wf_api::resolve_execution(&ctx, id).await {
        Ok(_) => next.run(req).await,
        Err(e) => crate::envelope::error_response(e).into_response(),
    }
}

/// Read the request directory for workspace-bound calls. `None` passes
/// through: the workspace resolver rejects it instead of guessing, so
/// directory-less requests fail loud instead of binding anywhere silently.
pub(crate) fn scope_dir(scope: &Option<DirectoryScope>) -> Option<&Path> {
    scope.as_ref().map(|scope| scope.directory.as_path())
}

/// Validate a client-supplied explicit file scope against the request
/// directory. Every absolute prefix and workdir must stay inside the
/// directory the client named: the request directory already passed the
/// allowlist, so containment here makes the allowlist a real ceiling
/// instead of a hint the body can override. Relative forms only pass
/// lexical validation (no escapes); they resolve against the execution
/// workdir inside the engine.
pub(crate) fn check_explicit_file_scope(
    scope: &DirectoryScope,
    file_scope: &wf_types::config::file_scope::ExecutionFileScope,
) -> Result<(), wf_api::ApiError> {
    use wf_types::config::file_scope::{scope_path_contains, validate_scope_path};
    let root = scope.directory.to_string_lossy().into_owned();
    let check_paths =
        |field: &str, paths: &Option<Vec<String>>| -> Result<(), wf_api::ApiError> {
            let Some(paths) = paths else {
                return Ok(());
            };
            for path in paths {
                validate_scope_path(path, field).map_err(wf_api::ApiError::Validation)?;
                if path.starts_with('/') && !scope_path_contains(&root, path) {
                    return Err(wf_api::ApiError::Validation(format!(
                        "{field} '{path}' is outside the request directory {}",
                        scope.directory.display()
                    )));
                }
            }
            Ok(())
        };
    check_paths("read_prefixes", &file_scope.read_prefixes)?;
    check_paths("write_prefixes", &file_scope.write_prefixes)?;
    check_paths("checkpoint_prefixes", &file_scope.checkpoint_prefixes)?;
    if let Some(workdir) = file_scope.workdir.as_deref() {
        validate_scope_path(workdir, "workdir").map_err(wf_api::ApiError::Validation)?;
        if workdir.starts_with('/') && !scope_path_contains(&root, workdir) {
            return Err(wf_api::ApiError::Validation(format!(
                "workdir '{workdir}' is outside the request directory {}",
                scope.directory.display()
            )));
        }
    }
    Ok(())
}

/// Symmetric ownership check between the request scope and the directory
/// stamped on an execution record at creation.
/// - Unowned record (`None`) is visible only to directory-less requests.
/// - Owned record is visible only to a request naming the same directory
///   (canonical comparison with a trimmed-string fallback).
/// - Every mismatch is invisible (callers map it to not-found, never to
///   forbidden, so directories cannot be probed as an oracle).
pub(crate) fn check_owner(scope: &Option<DirectoryScope>, directory: Option<&str>) -> bool {
    match (
        scope.as_ref(),
        directory.map(str::trim).filter(|d| !d.is_empty()),
    ) {
        (None, None) => true,
        (Some(request), Some(stored)) => {
            wf_common::workspace::same_directory(&request.directory.to_string_lossy(), stored)
        }
        _ => false,
    }
}

/// Ownership gate for execution by-id operations. A wrong directory is
/// indistinguishable from a wrong id: both surface as not-found, never as
/// forbidden, so directories cannot be probed as an oracle.
pub(crate) fn require_owner(
    scope: &Option<DirectoryScope>,
    directory: Option<&str>,
    entity_type: &str,
    id: &str,
) -> Result<(), wf_api::ApiError> {
    if check_owner(scope, directory) {
        Ok(())
    } else {
        Err(wf_api::ApiError::not_found(entity_type, id))
    }
}

/// Ownership gate for one workflow execution: unknown ids and directory
/// mismatches both fail as not-found before the handler touches the run.
pub(crate) async fn require_workflow_owner(
    ctx: &wf_api::ApiContext,
    scope: &Option<DirectoryScope>,
    id: &str,
) -> Result<(), wf_api::ApiError> {
    let owner = wf_api::workflow::execution::execution_owner(ctx, id).await?;
    let Some(directory) = owner else {
        return Err(wf_api::ApiError::not_found("execution", id));
    };
    require_owner(scope, directory.as_deref(), "execution", id)
}

/// Ownership gate for one agent loop execution: unknown ids and directory
/// mismatches both fail as not-found before the handler touches the run.
pub(crate) async fn require_agent_owner(
    ctx: &wf_api::ApiContext,
    scope: &Option<DirectoryScope>,
    id: &str,
) -> Result<(), wf_api::ApiError> {
    let owner = wf_api::agent::agent_execution::execution_owner(ctx, id).await?;
    let Some(directory) = owner else {
        return Err(wf_api::ApiError::execution_not_found(id));
    };
    if check_owner(scope, directory.as_deref()) {
        Ok(())
    } else {
        Err(wf_api::ApiError::execution_not_found(id))
    }
}

/// Ownership gate for one execution of either engine. Unknown ids and
/// directory mismatches both fail as not-found before the handler reads
/// the run.
pub(crate) async fn require_execution_owner(
    ctx: &wf_api::ApiContext,
    scope: &Option<DirectoryScope>,
    id: &str,
) -> Result<(), wf_api::ApiError> {
    match wf_api::resolve_execution(ctx, id).await {
        Ok(wf_api::ExecutionDomain::Workflow) => require_workflow_owner(ctx, scope, id).await,
        Ok(wf_api::ExecutionDomain::AgentLoop) => require_agent_owner(ctx, scope, id).await,
        Err(e) => Err(e),
    }
}

/// Visible subset of execution ids for scan-style reads (log / artifact /
/// search result pages). Unknown ids fail closed. Callers pass the distinct
/// ids of one page so ownership loads stay proportional to the page.
pub(crate) async fn visible_execution_ids(
    ctx: &wf_api::ApiContext,
    scope: &Option<DirectoryScope>,
    ids: impl IntoIterator<Item = String>,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut visible = HashSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        if require_execution_owner(ctx, scope, &id).await.is_ok() {
            visible.insert(id);
        }
    }
    visible
}

/// Owning directory of the request as an owned string for filters and
/// stamps. `None` scopes (directory-less requests) stay `None`, matching
/// the unowned-only rule.
pub(crate) fn scope_dir_string(scope: &Option<DirectoryScope>) -> Option<String> {
    scope
        .as_ref()
        .map(|scope| scope.directory.to_string_lossy().into_owned())
}

/// Read the request scope, rejecting directory-less requests.
pub(crate) fn require_scope(
    scope: &Option<DirectoryScope>,
) -> Result<&DirectoryScope, wf_api::ApiError> {
    scope
        .as_ref()
        .ok_or_else(|| wf_api::ApiError::Validation("request directory is required".to_string()))
}

/// Build the per-execution filesystem scope anchoring one execution on the
/// request directory. Callers apply it only when the request body carries
/// no explicit file scope, so explicit values always win.
pub(crate) fn scope_file_scope(
    scope: &DirectoryScope,
) -> wf_types::config::file_scope::ExecutionFileScope {
    let root = scope.directory.to_string_lossy().into_owned();
    wf_types::config::file_scope::ExecutionFileScope {
        enabled: Some(true),
        read_prefixes: Some(vec![root.clone()]),
        write_prefixes: Some(vec![root.clone()]),
        checkpoint_prefixes: Some(vec![root.clone()]),
        workdir: Some(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new().prefix(name).tempdir().unwrap()
    }

    fn config_with(roots: &[PathBuf]) -> DirectoryConfig {
        DirectoryConfig {
            allowed_roots: roots.to_vec(),
        }
    }

    #[test]
    fn header_wins_over_query() {
        let dir = temp_dir("wf-dir-header");
        let other = temp_dir("wf-dir-other");
        let mut headers = HeaderMap::new();
        headers.insert(
            DIRECTORY_HEADER,
            dir.path().to_string_lossy().to_string().parse().unwrap(),
        );
        let query = format!("directory={}", other.path().to_string_lossy());
        let raw = extract_raw(&headers, Some(&query));
        assert_eq!(raw.as_deref(), Some(dir.path().to_str().unwrap()));
    }

    #[test]
    fn query_covers_headerless_channels() {
        let dir = temp_dir("wf-dir-query");
        let headers = HeaderMap::new();
        let query = format!("directory={}", dir.path().to_string_lossy());
        let raw = extract_raw(&headers, Some(&query));
        assert_eq!(raw.as_deref(), Some(dir.path().to_str().unwrap()));
    }

    #[test]
    fn missing_inputs_yield_none() {
        assert_eq!(extract_raw(&HeaderMap::new(), None), None);
        assert_eq!(extract_raw(&HeaderMap::new(), Some("foo=1")), None);
    }

    #[test]
    fn query_decodes_percent_escapes() {
        let headers = HeaderMap::new();
        let raw = extract_raw(&headers, Some("directory=%2Ftmp%2Fa%20b"));
        assert_eq!(raw.as_deref(), Some("/tmp/a b"));
    }

    #[test]
    fn canonicalize_rejects_bad_inputs() {
        assert_eq!(canonicalize_dir("   ").unwrap_err(), DirectoryError::Empty);
        assert!(matches!(
            canonicalize_dir("relative/path").unwrap_err(),
            DirectoryError::NotAbsolute(_)
        ));
        assert!(matches!(
            canonicalize_dir("/definitely/not/here-wf-test").unwrap_err(),
            DirectoryError::NotFound(_)
        ));
    }

    #[test]
    fn canonicalize_accepts_real_directory() {
        let dir = temp_dir("wf-dir-real");
        let canonical = canonicalize_dir(dir.path().to_str().unwrap()).unwrap();
        assert!(canonical.is_absolute());
        assert!(canonical.is_dir());
    }

    #[test]
    fn default_workspace_serves_directory_less_requests() {
        let dir = temp_dir("wf-dir-default");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, dir.path()).unwrap();
        let scope = ctx.resolve(&HeaderMap::new(), None).unwrap().unwrap();
        assert_eq!(scope.directory, dir.path().canonicalize().unwrap());
        assert_eq!(ctx.default_scope(), Some(&scope));
    }

    #[test]
    fn default_workspace_rejects_unserved_directory() {
        let allowed = temp_dir("wf-dir-allowed");
        let denied = temp_dir("wf-dir-denied");
        let config = config_with(&[allowed.path().to_path_buf()]);
        assert!(DirectoryContext::with_default(config, denied.path()).is_err());
    }

    #[test]
    fn explicit_directory_wins_over_default() {
        let fallback = temp_dir("wf-dir-fallback");
        let explicit = temp_dir("wf-dir-explicit");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, fallback.path()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            DIRECTORY_HEADER,
            explicit
                .path()
                .to_string_lossy()
                .to_string()
                .parse()
                .unwrap(),
        );
        let scope = ctx.resolve(&headers, None).unwrap().unwrap();
        assert_eq!(scope.directory, explicit.path().canonicalize().unwrap());
    }

    #[test]
    fn explicit_scope_inside_directory_passes() {
        let dir = temp_dir("wf-dir-scope-ok");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, dir.path()).unwrap();
        let scope = ctx.default_scope().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let file_scope = wf_types::config::file_scope::ExecutionFileScope {
            read_prefixes: Some(vec![root.join("src").to_string_lossy().into_owned()]),
            write_prefixes: Some(vec![root.to_string_lossy().into_owned()]),
            checkpoint_prefixes: None,
            workdir: Some(root.to_string_lossy().into_owned()),
            enabled: None,
        };
        assert!(check_explicit_file_scope(scope, &file_scope).is_ok());
    }

    #[test]
    fn explicit_scope_outside_directory_fails() {
        let dir = temp_dir("wf-dir-scope-fence");
        let other = temp_dir("wf-dir-scope-other");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, dir.path()).unwrap();
        let scope = ctx.default_scope().unwrap();
        let outside = other.path().canonicalize().unwrap();
        let file_scope = wf_types::config::file_scope::ExecutionFileScope {
            read_prefixes: Some(vec![outside.to_string_lossy().into_owned()]),
            write_prefixes: None,
            checkpoint_prefixes: None,
            workdir: None,
            enabled: None,
        };
        assert!(check_explicit_file_scope(scope, &file_scope).is_err());
    }

    #[test]
    fn explicit_scope_sibling_prefix_fails() {
        let dir = temp_dir("wf-dir-scope-sib");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, dir.path()).unwrap();
        let scope = ctx.default_scope().unwrap();
        let root = dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let file_scope = wf_types::config::file_scope::ExecutionFileScope {
            read_prefixes: Some(vec![format!("{root}-sibling")]),
            write_prefixes: None,
            checkpoint_prefixes: None,
            workdir: None,
            enabled: None,
        };
        assert!(check_explicit_file_scope(scope, &file_scope).is_err());
    }

    #[test]
    fn owner_check_is_symmetric() {
        let dir = temp_dir("wf-dir-owner");
        let other = temp_dir("wf-dir-owner-other");
        let config = config_with(&[]);
        let ctx = DirectoryContext::with_default(config, dir.path()).unwrap();
        let scope = ctx.resolve(&HeaderMap::new(), None).ok().flatten();
        let root = dir.path().canonicalize().unwrap();
        let root = root.to_string_lossy().into_owned();
        let foreign = other.path().canonicalize().unwrap();
        let foreign = foreign.to_string_lossy().into_owned();
        assert!(check_owner(&None, None));
        assert!(check_owner(&scope, Some(&root)));
        assert!(!check_owner(&scope, None));
        assert!(!check_owner(&None, Some(&root)));
        assert!(!check_owner(&scope, Some(&foreign)));
        assert!(!check_owner(&scope, Some("")));
    }

    #[test]
    fn missing_directory_yields_no_scope() {
        let config = config_with(&[]);
        assert_eq!(resolve_scope(None, &config).unwrap(), None);
    }

    #[test]
    fn empty_allowlist_permits_explicit_directories() {
        let dir = temp_dir("wf-dir-open");
        let config = config_with(&[]);
        let scope = resolve_scope(Some(dir.path().to_str().unwrap()), &config)
            .unwrap()
            .unwrap();
        assert_eq!(scope.directory, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn allowlist_gates_explicit_directories() {
        let allowed = temp_dir("wf-dir-allowed");
        let denied = temp_dir("wf-dir-denied");
        let config = config_with(&[allowed.path().to_path_buf()]);
        let ok = resolve_scope(Some(allowed.path().to_str().unwrap()), &config)
            .unwrap()
            .unwrap();
        assert_eq!(ok.directory, allowed.path().canonicalize().unwrap());
        assert!(matches!(
            resolve_scope(Some(denied.path().to_str().unwrap()), &config).unwrap_err(),
            DirectoryError::Forbidden(_)
        ));
    }

    #[test]
    fn nested_directory_passes_parent_root() {
        let allowed = temp_dir("wf-dir-parent");
        let nested = allowed.path().join("child");
        std::fs::create_dir(&nested).unwrap();
        let config = config_with(&[allowed.path().to_path_buf()]);
        let scope = resolve_scope(Some(nested.to_str().unwrap()), &config).unwrap();
        assert!(scope.is_some());
    }

    #[test]
    fn missing_scope_fails_loud() {
        assert!(require_scope(&None).is_err());
        let dir = temp_dir("wf-dir-require");
        let scope = resolve_scope(Some(dir.path().to_str().unwrap()), &config_with(&[])).unwrap();
        assert_eq!(
            require_scope(&scope).unwrap().directory,
            dir.path().canonicalize().unwrap()
        );
    }

    fn enabled_ctx() -> std::sync::Arc<wf_api::ApiContext> {
        let ctx = wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
        );
        std::sync::Arc::new(
            ctx.with_user_git(wf_types::config::user_git::UserGitConfig {
                enabled: true,
                ..Default::default()
            }),
        )
    }

    #[tokio::test]
    async fn workspace_routes_reject_directory_less_requests() {
        use tower::ServiceExt;
        let app = crate::router::api_router_with_dirs(
            enabled_ctx(),
            std::sync::Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
            std::sync::Arc::new(DirectoryContext::new(DirectoryConfig::default())),
        );
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/fs/list")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn workspace_routes_serve_explicit_directories() {
        use tower::ServiceExt;
        let dir = temp_dir("wf-dir-served");
        let app = crate::router::api_router_with_dirs(
            enabled_ctx(),
            std::sync::Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
            std::sync::Arc::new(DirectoryContext::new(DirectoryConfig::default())),
        );
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/fs/list")
                    .header(DIRECTORY_HEADER, dir.path().to_str().unwrap())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[test]
    fn error_maps_to_api_taxonomy() {
        let api: wf_api::ApiError = DirectoryError::NotFound("/missing".to_string()).into();
        assert!(matches!(api, wf_api::ApiError::NotFound { .. }));
        let api: wf_api::ApiError = DirectoryError::Forbidden("/elsewhere".to_string()).into();
        assert!(matches!(api, wf_api::ApiError::Validation(_)));
    }
}
