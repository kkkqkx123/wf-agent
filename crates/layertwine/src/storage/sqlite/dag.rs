use crate::core::layer::Layer;
use crate::core::types::{CheckpointId, ContentId, LayerType, PartitionId};
use crate::storage::repository::{DagStore, LayerStore, MetadataStore};
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::params;
use std::collections::{HashMap, HashSet};

impl MetadataStore for SqliteStorage {
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO dag_store (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value.as_bytes(), now],
        )?;
        Ok(())
    }

    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT value FROM dag_store WHERE key = ?1")?;
        let result = stmt.query_row(rusqlite::params![key], |row| {
            let value: Vec<u8> = row.get(0)?;
            String::from_utf8(value)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        });
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::StorageError::Database(e)),
        }
    }
}

impl LayerStore for SqliteStorage {
    fn store_layer(&self, layer: &Layer) -> StorageResult<()> {
        let conn = self.conn.lock();
        let partition_ids_json = serde_json::to_vec(&layer.partitions)
            .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO layers (layer_type, partition_ids, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![layer.layer_type.name(), partition_ids_json, now, now,],
        )?;
        Ok(())
    }

    fn get_layer(&self, layer_type: &LayerType) -> StorageResult<Layer> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT layer_type, partition_ids FROM layers WHERE layer_type = ?1")?;

        let result = stmt.query_row(params![layer_type.name()], |row| {
            let _lt: String = row.get(0)?;
            let partition_ids_json: Vec<u8> = row.get(1)?;
            let partitions: Vec<PartitionId> = serde_json::from_slice(&partition_ids_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok(Layer {
                layer_type: layer_type.clone(),
                partitions,
            })
        })?;
        Ok(result)
    }

    fn list_layer_types(&self) -> StorageResult<Vec<LayerType>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT layer_type FROM layers ORDER BY layer_type")?;
        let types: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let result: Vec<LayerType> = types
            .iter()
            .filter_map(|s| LayerType::from_name(s))
            .collect();
        Ok(result)
    }

    fn delete_layer(&self, layer_type: &LayerType) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM layers WHERE layer_type = ?1",
            params![layer_type.name()],
        )?;
        Ok(())
    }
}

impl DagStore for SqliteStorage {
    fn store_dag_batch(
        &self,
        nodes: &[(CheckpointId, u64)],
        edges: &[(CheckpointId, CheckpointId)],
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        {
            let mut edge_stmt = tx.prepare(
                "INSERT OR IGNORE INTO dag_edges (parent_id, child_id) VALUES (?1, ?2)",
            )?;
            for (parent_id, child_id) in edges {
                edge_stmt.execute(rusqlite::params![&parent_id.0.to_vec(), &child_id.0.to_vec()])?;
            }
        }
        {
            let mut gen_stmt = tx.prepare(
                "INSERT OR REPLACE INTO dag_generations (node_id, generation) VALUES (?1, ?2)",
            )?;
            for (node_id, generation) in nodes {
                gen_stmt
                    .execute(rusqlite::params![&node_id.0.to_vec(), *generation as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn store_dag_edge(
        &self,
        parent_id: &CheckpointId,
        child_id: &CheckpointId,
        child_generation: u64,
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO dag_edges (parent_id, child_id) VALUES (?1, ?2)",
            rusqlite::params![&parent_id.0.to_vec(), &child_id.0.to_vec()],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO dag_generations (node_id, generation) VALUES (?1, ?2)",
            rusqlite::params![&child_id.0.to_vec(), child_generation as i64],
        )?;
        // Ensure parent node also exists in generations table
        conn.execute(
            "INSERT OR IGNORE INTO dag_generations (node_id, generation) VALUES (?1, ?2)",
            rusqlite::params![&parent_id.0.to_vec(), 0i64],
        )?;
        Ok(())
    }

    fn delete_dag_node(&self, node_id: &CheckpointId) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM dag_edges WHERE parent_id = ?1 OR child_id = ?1",
            rusqlite::params![&node_id.0.to_vec()],
        )?;
        conn.execute(
            "DELETE FROM dag_generations WHERE node_id = ?1",
            rusqlite::params![&node_id.0.to_vec()],
        )?;
        Ok(())
    }

    fn load_dag(&self) -> StorageResult<(
        HashMap<CheckpointId, HashSet<CheckpointId>>,
        HashMap<CheckpointId, u64>,
    )> {
        let conn = self.conn.lock();
        let mut nodes: HashMap<CheckpointId, HashSet<CheckpointId>> = HashMap::new();
        let mut generation: HashMap<CheckpointId, u64> = HashMap::new();

        // Load edges
        let mut edge_stmt = conn.prepare("SELECT parent_id, child_id FROM dag_edges")?;
        let edges = edge_stmt
            .query_map([], |row| {
                let parent_bytes: Vec<u8> = row.get(0)?;
                let child_bytes: Vec<u8> = row.get(1)?;
                let mut parent_arr = [0u8; 32];
                let mut child_arr = [0u8; 32];
                parent_arr.copy_from_slice(&parent_bytes);
                child_arr.copy_from_slice(&child_bytes);
                Ok((ContentId(parent_arr), ContentId(child_arr)))
            })
            .map_err(crate::StorageError::Database)?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();

        for (parent_id, child_id) in &edges {
            nodes.entry(*parent_id).or_default().insert(*child_id);
            nodes.entry(*child_id).or_default();
        }

        // Load generations
        let mut gen_stmt = conn.prepare("SELECT node_id, generation FROM dag_generations")?;
        let gen_rows = gen_stmt
            .query_map([], |row| {
                let node_bytes: Vec<u8> = row.get(0)?;
                let gen: i64 = row.get(1)?;
                let mut node_arr = [0u8; 32];
                node_arr.copy_from_slice(&node_bytes);
                Ok((ContentId(node_arr), gen as u64))
            })
            .map_err(crate::StorageError::Database)?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();

        for (node_id, gen) in &gen_rows {
            generation.insert(*node_id, *gen);
        }

        Ok((nodes, generation))
    }

    fn dag_has_node(&self, node_id: &CheckpointId) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dag_generations WHERE node_id = ?1",
            rusqlite::params![&node_id.0.to_vec()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}
