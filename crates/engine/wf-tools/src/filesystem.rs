//! Filesystem tool handlers: shared entry points for the registered file
//! operations. The implementation is split by responsibility:
//! [`config`] holds the tool configuration, [`scope`] the execution scope
//! gates, [`ignore`] the discovery and capture ignore rules,
//! [`handlers_read`] the read-side operations, [`handlers_write`] the
//! write-side operations and [`patch_ops`] the patch/diff operations.

mod config;
mod handlers_read;
mod handlers_write;
mod ignore;
mod patch_ops;
mod scope;

#[cfg(test)]
mod tests;

pub use config::FsToolConfig;

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use crate::protect::ProtectController;

pub struct FsToolHandlers {
    config: FsToolConfig,
    protect: Option<ProtectController>,
}

impl FsToolHandlers {
    pub fn new(config: FsToolConfig) -> Self {
        Self {
            config,
            protect: None,
        }
    }

    pub fn with_protect(mut self, protect: ProtectController) -> Self {
        self.protect = Some(protect);
        self
    }

    fn is_write_protected(&self, path: &Path) -> bool {
        self.protect
            .as_ref()
            .map(|p| p.is_write_protected(path.to_string_lossy().as_ref()))
            .unwrap_or(false)
    }

    /// Construct the handler closure for a given tool name. Write-capable
    /// tools keep the execution context so precise change notifications
    /// carry the current execution id; registration itself is unchanged.
    pub fn handler(
        &self,
        tool_name: &'static str,
    ) -> ToolResult<crate::executor::stateless::StatelessHandler> {
        let this = self.clone();
        let handler = Arc::new(
            move |params: &Value, ctx: &ToolExecutionContext| match tool_name {
                "read_file" => this.read_file(params, ctx),
                "write_file" => this.write_file(params, ctx),
                "edit_file" => this.edit_file(params, ctx),
                "apply_patch" => this.apply_patch(params, ctx),
                "apply_diff" => this.apply_diff(params, ctx),
                "list_files" => this.list_files(params, ctx),
                "grep_search" => this.grep_search(params, ctx),
                "glob_search" => this.glob_search(params, ctx),
                _ => Err(ToolError::NotFound(format!(
                    "No filesystem handler for tool '{}'",
                    tool_name
                ))),
            },
        );
        Ok(handler)
    }
}

impl Clone for FsToolHandlers {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            protect: self.protect.clone(),
        }
    }
}

fn require_string<'a>(parameters: &'a Value, key: &str) -> ToolResult<&'a str> {
    parameters.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
        ToolError::ValidationFailed(format!("Missing or invalid '{}' parameter", key))
    })
}
