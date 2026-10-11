//! Execution scope gates: path resolution against the execution workdir,
//! read/write prefix checks, symlink escape checks and the checkpoint
//! write gate.

use std::path::{Path, PathBuf};

use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use wf_types::config::file_scope::{normalize_scope_path, scope_path_contains};

use super::FsToolHandlers;

impl FsToolHandlers {
    pub(super) fn scoped_base(&self, ctx: &ToolExecutionContext) -> Option<PathBuf> {
        ctx.file_scope
            .as_ref()
            .and_then(|s| s.workdir.as_ref().map(PathBuf::from))
            .or_else(|| self.config.workspace_dir.clone())
    }

    pub(super) fn resolve_scoped(&self, path: &str, ctx: &ToolExecutionContext) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else if let Some(base) = self.scoped_base(ctx) {
            base.join(candidate)
        } else {
            candidate.to_path_buf()
        }
    }

    fn scope_allows_violation(&self, want_write: bool) -> bool {
        use wf_types::script::sandbox::SandboxMode;
        // Writes stay fail-closed in every mode: a lenient write would land
        // on disk while the checkpoint layer drops it as out of scope,
        // leaving an unrestorable side effect behind a success message.
        if want_write {
            return false;
        }
        matches!(
            self.config.scope_mode,
            Some(SandboxMode::Lenient) | Some(SandboxMode::Disabled)
        )
    }

    pub(super) fn scope_violation(
        &self,
        tool: &str,
        ctx: &ToolExecutionContext,
        message: String,
        fallback: PathBuf,
        want_write: bool,
    ) -> ToolResult<PathBuf> {
        if self.scope_allows_violation(want_write) {
            tracing::warn!(
                execution = %ctx.execution_id,
                error = %message,
                "filesystem scope violation allowed by lenient mode; checkpoint will not endorse the write"
            );
            return Ok(fallback);
        }
        Err(ToolError::scope_violation(tool, message))
    }

    pub(super) fn scope_guidance(&self, ctx: &ToolExecutionContext, kind: &str) -> String {
        match ctx
            .file_scope
            .as_ref()
            .and_then(|s| s.workdir.as_deref())
        {
            Some(workdir) => format!(
                " Work inside '{workdir}' and keep paths under the allowed {kind} prefixes of this execution."
            ),
            None => format!(
                " Keep paths under the allowed {kind} prefixes of this execution."
            ),
        }
    }

    pub(super) fn ensure_in_scope(
        &self,
        tool: &str,
        resolved: &Path,
        ctx: &ToolExecutionContext,
        want_write: bool,
    ) -> ToolResult<PathBuf> {
        let Some(scope) = ctx.file_scope.as_ref() else {
            return Ok(resolved.to_path_buf());
        };
        if !scope.enabled {
            return Ok(resolved.to_path_buf());
        }
        let kind = if want_write { "write" } else { "read" };
        if !resolved.is_absolute() {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %resolved.display(),
                "filesystem scope violation: relative path without base"
            );
            let guidance = self.scope_guidance(ctx, kind);
            return self.scope_violation(
                tool,
                ctx,
                format!(
                    "path '{}' outside allowed {} prefixes: relative path without base.{}",
                    resolved.display(),
                    kind,
                    guidance
                ),
                resolved.to_path_buf(),
                want_write,
            );
        }
        let lexical = normalize_scope_path(&resolved.to_string_lossy());
        let bounds = if want_write {
            &scope.write_prefixes
        } else {
            &scope.read_prefixes
        };
        let lexically_inside = bounds
            .iter()
            .any(|bound| scope_path_contains(bound, &lexical));
        if !lexically_inside {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %resolved.display(),
                want_write = want_write,
                "filesystem scope violation: outside allowed prefixes"
            );
            let guidance = self.scope_guidance(ctx, kind);
            return self.scope_violation(
                tool,
                ctx,
                format!(
                    "path '{}' outside allowed {} prefixes.{}",
                    resolved.display(),
                    kind,
                    guidance
                ),
                PathBuf::from(&lexical),
                want_write,
            );
        }
        if let Ok(physical) = resolved.canonicalize() {
            let physical_str = physical.to_string_lossy().to_string();
            let physically_inside = bounds
                .iter()
                .any(|bound| scope_path_contains(bound, &physical_str));
            if !physically_inside {
                tracing::warn!(
                    execution = %ctx.execution_id,
                    path = %resolved.display(),
                    physical = %physical.display(),
                    "filesystem scope violation: symlink escapes allowed prefixes"
                );
                let guidance = self.scope_guidance(ctx, kind);
                return self.scope_violation(
                    tool,
                    ctx,
                    format!(
                        "path '{}' escapes allowed {} prefixes through symlink.{}",
                        resolved.display(),
                        kind,
                        guidance
                    ),
                    physical.clone(),
                    want_write,
                );
            }
            return self.finish_write_gate(tool, &physical, ctx, want_write, &lexical);
        }
        if let Some(parent) = resolved.parent() {
            if !parent.as_os_str().is_empty() {
                if let Ok(physical_parent) = parent.canonicalize() {
                    if let Some(name) = resolved.file_name() {
                        let physical = physical_parent.join(name);
                        let physical_str = physical.to_string_lossy().to_string();
                        let physically_inside = bounds
                            .iter()
                            .any(|bound| scope_path_contains(bound, &physical_str));
                        if !physically_inside {
                            tracing::warn!(
                                execution = %ctx.execution_id,
                                path = %resolved.display(),
                                physical = %physical.display(),
                                "filesystem scope violation: parent symlink escapes prefixes"
                            );
                            let guidance = self.scope_guidance(ctx, kind);
                            return self.scope_violation(
                                tool,
                                ctx,
                                format!(
                                    "path '{}' escapes allowed {} prefixes through symlink.{}",
                                    resolved.display(),
                                    kind,
                                    guidance
                                ),
                                physical.clone(),
                                want_write,
                            );
                        }
                        return self.finish_write_gate(tool, &physical, ctx, want_write, &lexical);
                    }
                }
            }
        }
        self.finish_write_gate(tool, &PathBuf::from(&lexical), ctx, want_write, &lexical)
    }

    // Instance scope, global protection and ignore rules intersect as a
    // tightening: a path must pass scope gating and must not be protected
    // or ignored. Instance scope never relaxes a global denial.
    //
    // The discovery anchor prefers the execution workdir over the static
    // workspace dir so workspace ignore files resolve against the same
    // root the execution was scoped to.
    pub(super) fn scope_anchor(&self, ctx: &ToolExecutionContext) -> PathBuf {
        ctx.file_scope
            .as_ref()
            .and_then(|s| s.workdir.as_ref().map(PathBuf::from))
            .or_else(|| self.config.workspace_dir.clone())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Read gate for callers without a full handler (service-side paths):
    /// resolves against the execution workdir and applies the same read
    /// prefixes plus opportunistic physical check as the file tools.
    pub(crate) fn ensure_read_scoped(
        &self,
        tool: &str,
        raw: &str,
        ctx: &ToolExecutionContext,
    ) -> ToolResult<PathBuf> {
        let resolved = self.resolve_scoped(raw, ctx);
        self.ensure_in_scope(tool, &resolved, ctx, false)
    }
}

pub(super) fn scope_allows_read(
    path: &Path,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    if !scope.enabled {
        return true;
    }
    let lexical = wf_types::config::file_scope::normalize_scope_path(&path.to_string_lossy());
    let lexically_inside = scope
        .read_prefixes
        .iter()
        .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &lexical));
    if !lexically_inside {
        return false;
    }
    if let Ok(physical) = path.canonicalize() {
        let physical_str = physical.to_string_lossy().to_string();
        return scope
            .read_prefixes
            .iter()
            .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &physical_str));
    }
    true
}
