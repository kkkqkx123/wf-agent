use crate::core::file_move::FileMove;
use crate::storage::repository::FileMoveStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, Row};

fn row_to_file_move(row: &Row) -> Result<FileMove, rusqlite::Error> {
    let from_path: String = row.get(0)?;
    let to_path: String = row.get(1)?;
    let timestamp: i64 = row.get(2)?;
    let source: String = row.get(3)?;
    Ok(FileMove {
        from_path,
        to_path,
        timestamp,
        source,
    })
}

impl FileMoveStore for SqliteStorage {
    fn store_file_move(&self, file_move: &FileMove) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO file_moves (from_path, to_path, timestamp, source)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                file_move.from_path,
                file_move.to_path,
                file_move.timestamp,
                file_move.source,
            ],
        )?;
        Ok(())
    }

    fn get_moves_from(&self, from_path: &str) -> StorageResult<Vec<FileMove>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT from_path, to_path, timestamp, source
             FROM file_moves WHERE from_path = ?1 ORDER BY timestamp ASC",
        )?;
        let moves = stmt.query_map(params![from_path], row_to_file_move)?;
        Ok(moves.collect::<Result<Vec<_>, _>>()?)
    }

    fn get_moves_to(&self, to_path: &str) -> StorageResult<Vec<FileMove>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT from_path, to_path, timestamp, source
             FROM file_moves WHERE to_path = ?1 ORDER BY timestamp ASC",
        )?;
        let moves = stmt.query_map(params![to_path], row_to_file_move)?;
        Ok(moves.collect::<Result<Vec<_>, _>>()?)
    }

    fn trace_rename_chain(&self, path: &str) -> StorageResult<Vec<FileMove>> {
        let mut chain = Vec::new();
        let mut current = path.to_string();
        // Walk backwards: find moves where to_path == current, then follow from_path
        loop {
            let found = {
                let conn = self.conn.lock();
                let mut stmt = conn.prepare(
                    "SELECT from_path, to_path, timestamp, source
                     FROM file_moves WHERE to_path = ?1 ORDER BY timestamp DESC LIMIT 1",
                )?;
                stmt.query_row(params![current], row_to_file_move).ok()
            };

            match found {
                Some(m) => {
                    current = m.from_path.clone();
                    chain.push(m);
                }
                None => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_storage;

    #[test]
    fn test_store_and_get_file_move() {
        let storage = setup_storage();
        let m = FileMove::new(
            "old/path.txt".to_string(),
            "new/path.txt".to_string(),
            "manual".to_string(),
        );
        storage.store_file_move(&m).unwrap();

        let moves_from = storage.get_moves_from("old/path.txt").unwrap();
        assert_eq!(moves_from.len(), 1);
        assert_eq!(moves_from[0].to_path, "new/path.txt");

        let moves_to = storage.get_moves_to("new/path.txt").unwrap();
        assert_eq!(moves_to.len(), 1);
        assert_eq!(moves_to[0].from_path, "old/path.txt");
    }

    #[test]
    fn test_trace_rename_chain() {
        let storage = setup_storage();
        // a.txt -> b.txt -> c.txt
        storage
            .store_file_move(&FileMove::new(
                "a.txt".to_string(),
                "b.txt".to_string(),
                "manual".to_string(),
            ))
            .unwrap();
        storage
            .store_file_move(&FileMove::new(
                "b.txt".to_string(),
                "c.txt".to_string(),
                "agent:loop-1".to_string(),
            ))
            .unwrap();

        let chain = storage.trace_rename_chain("c.txt").unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].from_path, "a.txt");
        assert_eq!(chain[0].to_path, "b.txt");
        assert_eq!(chain[1].from_path, "b.txt");
        assert_eq!(chain[1].to_path, "c.txt");
    }

    #[test]
    fn test_trace_rename_chain_no_moves() {
        let storage = setup_storage();
        let chain = storage.trace_rename_chain("unknown.txt").unwrap();
        assert!(chain.is_empty());
    }
}
