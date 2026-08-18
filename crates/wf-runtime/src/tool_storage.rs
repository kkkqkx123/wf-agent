//! Bridge between the runtime storage layer and the tool registry
//! persistence contract (`wf_tools::registry::ToolStorage`): full tool
//! definitions saved through the registry land in the same backend as every
//! other entity, so tools registered at runtime survive restarts.

use async_trait::async_trait;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::ToolDefinitionStorage;
use wf_storage::backend::StorageBackend;
use wf_tools::error::{ToolError, ToolResult};
use wf_types::tool::Tool;

/// `wf_tools::registry::ToolStorage` implementation backed by the
/// `tool_definition` entity store of the runtime storage context.
pub struct StorageToolBridge {
    storage: ToolDefinitionStorage<StorageBackend>,
}

impl StorageToolBridge {
    pub fn new(storage: ToolDefinitionStorage<StorageBackend>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl wf_tools::registry::ToolStorage for StorageToolBridge {
    async fn load_tools(&self) -> ToolResult<Vec<Tool>> {
        self.storage
            .list(None)
            .await
            .map_err(|e| ToolError::Internal(format!("load tools from storage: {e}")))
    }

    async fn save_tool(&self, tool: &Tool) -> ToolResult<()> {
        self.storage
            .save(tool)
            .await
            .map_err(|e| ToolError::Internal(format!("persist tool {}: {e}", tool.id)))
    }

    async fn delete_tool(&self, tool_id: &str) -> ToolResult<()> {
        self.storage
            .delete(tool_id)
            .await
            .map_err(|e| ToolError::Internal(format!("delete tool {tool_id}: {e}")))?;
        Ok(())
    }
}
