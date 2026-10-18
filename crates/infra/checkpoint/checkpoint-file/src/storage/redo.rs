//! Durable redo-stack storage: the undone commit ids per actor, persisted
//! as one JSON blob keyed by actor id. Undo moves an actor's edit ref back
//! and records the removed ids here so redo can replay them across restarts.

use super::connection::SqliteStorage;
use super::repository::StorageResult;
use checkpoint_base::error::CheckpointError;

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

/// JSON array of commit ids; an undecodable blob degrades to an empty stack
/// rather than failing startup.
fn encode(entries: &[String]) -> Vec<u8> {
    serde_json::to_vec(entries).unwrap_or_else(|_| b"[]".to_vec())
}

fn decode(bytes: &[u8]) -> Vec<String> {
    serde_json::from_slice(bytes).unwrap_or_default()
}

impl SqliteStorage {
    /// Persist the full redo stack for one actor, replacing any prior value.
    pub fn store_redo_stack(&self, actor: &str, entries: &[String]) -> StorageResult<()> {
        self.with_conn(|conn| {
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "INSERT OR REPLACE INTO redo_stacks (actor, entries, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![actor, encode(entries), now],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    /// Drop the persisted redo stack for one actor.
    pub fn clear_stored_redo_stack(&self, actor: &str) -> StorageResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM redo_stacks WHERE actor = ?1",
                rusqlite::params![actor],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    /// Load every persisted redo stack as `(actor, entries)`. Startup
    /// rebuilds the in-memory mirrors from this exactly once.
    pub fn load_all_redo_stacks(&self) -> StorageResult<Vec<(String, Vec<String>)>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT actor, entries FROM redo_stacks")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |row| {
                    let actor: String = row.get(0)?;
                    let entries: Vec<u8> = row.get(1)?;
                    Ok((actor, entries))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            Ok(rows
                .into_iter()
                .map(|(actor, entries)| (actor, decode(&entries)))
                .collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redo_stack_round_trips_through_sqlite() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        storage
            .store_redo_stack("agent:a", &["c1".to_string(), "c2".to_string()])
            .unwrap();
        let all = storage.load_all_redo_stacks().unwrap();
        assert_eq!(
            all,
            vec![(
                "agent:a".to_string(),
                vec!["c1".to_string(), "c2".to_string()]
            )]
        );

        storage
            .store_redo_stack("agent:a", &["c3".to_string()])
            .unwrap();
        assert_eq!(
            storage.load_all_redo_stacks().unwrap(),
            vec![("agent:a".to_string(), vec!["c3".to_string()])]
        );

        storage.clear_stored_redo_stack("agent:a").unwrap();
        assert!(storage.load_all_redo_stacks().unwrap().is_empty());
    }
}
