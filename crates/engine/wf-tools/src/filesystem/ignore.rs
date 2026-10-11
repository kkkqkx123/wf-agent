//! Discovery ignore rules plus the checkpoint capture verdict that marks
//! write targets which can never be restored.

use std::path::Path;

use serde_json::Value;

use crate::executor::trait_def::ToolExecutionContext;
use crate::ignore::{IgnoreController, IgnoreMode};

use super::FsToolHandlers;

impl FsToolHandlers {
    pub(super) fn ignore_controller(&self, ctx: &ToolExecutionContext) -> Option<IgnoreController> {
        if !self.config.enable_ignore {
            return None;
        }
        let anchor = self.scope_anchor(ctx);
        Some(
            IgnoreController::new(&anchor.to_string_lossy(), IgnoreMode::All)
                .with_workspace_gitignore(&anchor),
        )
    }

    pub(super) fn include_ignored(parameters: &Value) -> bool {
        parameters
            .get("include_ignored")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub(super) fn ignore_note(count: usize) -> String {
        format!(
            "\nNote: {count} entries hidden by ignore rules were skipped; read an explicit path directly to inspect them."
        )
    }

    /// Checkpoint capture verdict for a write target: hardcoded excludes
    /// plus the workspace ignore files, resolved against the execution
    /// anchor. A capture-ignored write lands on disk but can never be
    /// restored, so callers mark the success text instead of implying
    /// durability. Paths outside the anchor cannot be evaluated and stay
    /// unmarked; scope gating already passed them.
    pub(super) fn capture_ignored(&self, ctx: &ToolExecutionContext, path: &Path) -> bool {
        let anchor = self.scope_anchor(ctx);
        if !anchor.is_absolute() {
            return false;
        }
        let Ok(rel) = path.strip_prefix(&anchor) else {
            return false;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            return false;
        }
        if wf_checkpoint::scan::is_hardcoded_ignored(&rel) {
            return true;
        }
        // The search build walks the anchor once per write; writes are
        // rare next to reads, so freshness wins over caching here.
        let search = wf_checkpoint::scan::collect_gitignore_search(&anchor);
        wf_checkpoint::scan::gitignore_ignored(&search, &rel)
    }

    pub(super) fn unrestorable_note(
        &self,
        ctx: &ToolExecutionContext,
        path: &Path,
    ) -> Option<&'static str> {
        if !self.capture_ignored(ctx, path) {
            return None;
        }
        tracing::warn!(
            execution = %ctx.execution_id,
            path = %path.display(),
            "filesystem write target is ignored by checkpoint capture and cannot be restored"
        );
        Some("\nNote: this path is ignored by checkpoint capture and cannot be restored.")
    }
}
