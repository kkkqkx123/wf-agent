use crate::core::edit_session::EditSession;
use crate::core::types::{DeltaId, EditSessionId};
use crate::storage::repository::EditSessionStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, OptionalExtension, Row};

fn row_to_session(row: &Row) -> Result<EditSession, rusqlite::Error> {
    let id_bytes: Vec<u8> = row.get(0)?;
    let id = uuid::Uuid::from_slice(&id_bytes)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let label: Option<String> = row.get(1)?;
    let created_at: i64 = row.get(2)?;
    Ok(EditSession {
        id,
        delta_ids: Vec::new(),
        label,
        created_at,
    })
}

impl EditSessionStore for SqliteStorage {
    fn store_session(&self, session: &EditSession) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO edit_sessions (id, label, created_at)
             VALUES (?1, ?2, ?3)",
            params![
                session.id.as_bytes().to_vec(),
                session.label,
                session.created_at,
            ],
        )?;
        for (seq, delta_id) in session.delta_ids.iter().enumerate() {
            conn.execute(
                "INSERT OR IGNORE INTO delta_sessions (delta_id, session_id, seq)
                 VALUES (?1, ?2, ?3)",
                params![
                    &delta_id.0.to_vec(),
                    session.id.as_bytes().to_vec(),
                    seq as i64,
                ],
            )?;
        }
        Ok(())
    }

    fn get_session(&self, id: &EditSessionId) -> StorageResult<EditSession> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at FROM edit_sessions WHERE id = ?1",
        )?;
        let mut session = stmt.query_row(params![id.as_bytes().to_vec()], row_to_session)?;
        session.delta_ids = self.get_session_deltas_inner(&conn, id)?;
        Ok(session)
    }

    fn list_sessions(&self) -> StorageResult<Vec<EditSession>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at FROM edit_sessions ORDER BY created_at DESC",
        )?;
        let sessions = stmt.query_map([], row_to_session)?;
        let mut result = Vec::new();
        for s in sessions {
            let mut session = s?;
            session.delta_ids = self.get_session_deltas_inner(&conn, &session.id)?;
            result.push(session);
        }
        Ok(result)
    }

    fn get_session_deltas(&self, session_id: &EditSessionId) -> StorageResult<Vec<DeltaId>> {
        let conn = self.conn.lock();
        self.get_session_deltas_inner(&conn, session_id)
    }

    fn get_delta_session(&self, delta_id: &DeltaId) -> StorageResult<Option<EditSession>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT es.id, es.label, es.created_at
             FROM edit_sessions es
             JOIN delta_sessions ds ON es.id = ds.session_id
             WHERE ds.delta_id = ?1",
        )?;
        let result = stmt
            .query_row(params![&delta_id.0.to_vec()], row_to_session)
            .optional()?;
        Ok(result)
    }

    fn delete_session(&self, id: &EditSessionId) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM delta_sessions WHERE session_id = ?1",
            params![id.as_bytes().to_vec()],
        )?;
        conn.execute(
            "DELETE FROM edit_sessions WHERE id = ?1",
            params![id.as_bytes().to_vec()],
        )?;
        Ok(())
    }
}

impl SqliteStorage {
    fn get_session_deltas_inner(
        &self,
        conn: &rusqlite::Connection,
        session_id: &EditSessionId,
    ) -> StorageResult<Vec<DeltaId>> {
        use crate::core::types::ContentId;
        let mut stmt = conn.prepare(
            "SELECT delta_id FROM delta_sessions WHERE session_id = ?1 ORDER BY seq",
        )?;
        let delta_ids = stmt
            .query_map(params![session_id.as_bytes().to_vec()], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(ContentId(arr))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(delta_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::SourceType;
    use crate::storage::repository::DeltaStore;
    use crate::test_utils::setup_storage;

    #[test]
    fn test_store_and_get_session() {
        let storage = setup_storage();
        let session = EditSession::new(Some("test session".to_string()));
        storage.store_session(&session).unwrap();

        let retrieved = storage.get_session(&session.id).unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.label, Some("test session".to_string()));
    }

    #[test]
    fn test_session_with_deltas() {
        use crate::core::delta::Delta;
        use crate::core::file_node::FileNode;
        use crate::core::types::LineDiff;

        let storage = setup_storage();
        let file_node = FileNode::new(std::path::PathBuf::from("test.txt"), b"hello");
        let empty_diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node, empty_diff, SourceType::Manual);
        storage.store_delta(&delta).unwrap();

        let mut session = EditSession::new(None);
        session.add_delta(delta.id);
        storage.store_session(&session).unwrap();

        let retrieved = storage.get_session(&session.id).unwrap();
        assert_eq!(retrieved.delta_ids.len(), 1);
        assert_eq!(retrieved.delta_ids[0], delta.id);
    }

    #[test]
    fn test_list_sessions() {
        let storage = setup_storage();
        let s1 = EditSession::new(Some("s1".to_string()));
        let s2 = EditSession::new(Some("s2".to_string()));
        storage.store_session(&s1).unwrap();
        storage.store_session(&s2).unwrap();

        let sessions = storage.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let storage = setup_storage();
        let session = EditSession::new(None);
        storage.store_session(&session).unwrap();
        storage.delete_session(&session.id).unwrap();

        let result = storage.get_session(&session.id);
        assert!(result.is_err());
    }
}
