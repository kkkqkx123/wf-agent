use std::collections::HashMap;
use std::sync::Arc;

pub trait GitCheckpointAdapter: Send + Sync {
    fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> impl std::future::Future<Output = Result<(), LayertwineError>> + Send;

    fn get_checkpoint(&self, checkpoint_id: &str)
        -> impl std::future::Future<Output = Result<Option<Vec<u8>>, LayertwineError>> + Send;

    fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<String>, LayertwineError>> + Send;

    fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> impl std::future::Future<Output = Result<(), LayertwineError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum LayertwineError {
    #[error("git operation failed: {0}")]
    Git(String),

    #[error("checkpoint not found: {id}")]
    NotFound { id: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct InMemoryGitAdapter {
    branches: tokio::sync::RwLock<HashMap<String, Vec<u8>>>,
    metadata: tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>,
}

impl InMemoryGitAdapter {
    pub fn new() -> Self {
        Self {
            branches: tokio::sync::RwLock::new(HashMap::new()),
            metadata: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryGitAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GitCheckpointAdapter for InMemoryGitAdapter {
    async fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        meta: &HashMap<String, String>,
    ) -> Result<(), LayertwineError> {
        let mut branches = self.branches.write().await;
        branches.insert(checkpoint_id.to_string(), data.to_vec());
        let mut metadata = self.metadata.write().await;
        metadata.insert(checkpoint_id.to_string(), meta.clone());
        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, LayertwineError> {
        let branches = self.branches.read().await;
        Ok(branches.get(checkpoint_id).cloned())
    }

    async fn list_checkpoints(
        &self,
        _parent_id: Option<&str>,
    ) -> Result<Vec<String>, LayertwineError> {
        let branches = self.branches.read().await;
        Ok(branches.keys().cloned().collect())
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), LayertwineError> {
        let mut branches = self.branches.write().await;
        let mut metadata = self.metadata.write().await;

        for (id, data, meta) in items {
            branches.insert(id.clone(), data.clone());
            metadata.insert(id.clone(), meta.clone());
        }

        Ok(())
    }
}

pub struct LayertwineCheckpointBridge<T: GitCheckpointAdapter> {
    adapter: Arc<T>,
}

impl<T: GitCheckpointAdapter> LayertwineCheckpointBridge<T> {
    pub fn new(adapter: Arc<T>) -> Self {
        Self { adapter }
    }

    pub async fn save(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), LayertwineError> {
        let mut meta = HashMap::new();
        meta.insert("entityType".to_string(), entity_type.to_string());
        meta.insert("entityId".to_string(), entity_id.to_string());
        meta.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );
        self.adapter
            .save_checkpoint(checkpoint_id, data, &meta)
            .await
    }

    pub async fn load(&self, checkpoint_id: &str) -> Result<Option<Vec<u8>>, LayertwineError> {
        self.adapter.get_checkpoint(checkpoint_id).await
    }

    pub async fn list(&self) -> Result<Vec<String>, LayertwineError> {
        self.adapter.list_checkpoints(None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_and_load_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"checkpoint data", "workflow", "exec-1")
            .await
            .unwrap();

        let data = bridge.load("cp-1").await.unwrap();
        assert_eq!(data, Some(b"checkpoint data".to_vec()));
    }

    #[tokio::test]
    async fn load_missing_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        let data = bridge.load("nonexistent").await.unwrap();
        assert!(data.is_none());
    }

    #[tokio::test]
    async fn list_checkpoints() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"data-1", "workflow", "exec-1")
            .await
            .unwrap();
        bridge
            .save("cp-2", b"data-2", "workflow", "exec-2")
            .await
            .unwrap();

        let list = bridge.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn batch_save() {
        let adapter = Arc::new(InMemoryGitAdapter::new());

        let items = vec![
            (
                "cp-1".to_string(),
                b"data-1".to_vec(),
                HashMap::from([("entityId".to_string(), "e1".to_string())]),
            ),
            (
                "cp-2".to_string(),
                b"data-2".to_vec(),
                HashMap::from([("entityId".to_string(), "e2".to_string())]),
            ),
        ];

        adapter.batch_save(&items).await.unwrap();

        assert!(adapter.get_checkpoint("cp-1").await.unwrap().is_some());
        assert!(adapter.get_checkpoint("cp-2").await.unwrap().is_some());
    }
}
