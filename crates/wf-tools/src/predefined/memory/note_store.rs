//! Session-note storage: SQLite file backend (feature `sqlite`) with an
//! in-memory fallback when the feature is disabled.
//!
//! The data model (`NoteEntry`):
//! id / timestamp / category / content / summary / tokenCount / createdAt /
//! updatedAt. The default database path follows the dbPath convention
//! (`<workspace>/data/session-notes.db`).

use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;

/// A single session note entry.
#[derive(Debug, Clone, Serialize)]
pub struct NoteEntry {
    pub id: String,
    pub timestamp: String,
    pub category: String,
    pub content: String,
    pub summary: String,
    pub token_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Payload for creating a note.
pub struct NewNote {
    pub category: String,
    pub content: String,
    pub summary: String,
    pub token_count: i64,
    pub timestamp: String,
}

/// Partial update applied to an existing note; `None` fields keep their
/// original values.
pub struct NotePatch {
    pub category: Option<String>,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub token_count: Option<i64>,
    pub timestamp: Option<String>,
}

/// Storage backend for session notes.
pub trait SessionNoteStore: Send + Sync {
    /// Insert a note and return the saved entry with its generated id.
    fn save(&self, session_id: &str, note: NewNote) -> NoteEntry;
    /// Load a single note by id within the session.
    fn get(&self, session_id: &str, note_id: &str) -> Option<NoteEntry>;
    /// Apply a partial update; returns `None` when the note does not exist.
    fn update(&self, session_id: &str, note_id: &str, patch: NotePatch) -> Option<NoteEntry>;
    /// List notes, newest first; optionally filtered by category.
    fn list(&self, session_id: &str, category: Option<&str>) -> Vec<NoteEntry>;
    /// Case-insensitive substring search over content and summary.
    fn search(&self, session_id: &str, term: &str) -> Vec<NoteEntry>;
    /// Delete a note; returns whether a note was removed.
    fn delete(&self, session_id: &str, note_id: &str) -> bool;
}

/// Build a store for the given database path. With the `sqlite` feature the
/// path resolves to a SQLite file; without it the store degrades to memory.
pub fn open_store(db_path: &str) -> Arc<dyn SessionNoteStore> {
    #[cfg(feature = "sqlite")]
    {
        if let Ok(store) = SqliteNoteStore::open(db_path) {
            return Arc::new(store);
        }
        tracing::warn!(
            db_path,
            "session-note sqlite store failed to open; falling back to memory"
        );
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = db_path;
    }
    Arc::new(InMemoryNoteStore::new())
}

/// In-memory fallback backend (used when the `sqlite` feature is disabled or
/// the database file cannot be opened).
struct InMemoryNoteStore {
    notes: DashMap<String, Vec<NoteEntry>>,
}

impl InMemoryNoteStore {
    fn new() -> Self {
        Self {
            notes: DashMap::new(),
        }
    }
}

impl SessionNoteStore for InMemoryNoteStore {
    fn save(&self, session_id: &str, note: NewNote) -> NoteEntry {
        let entry = NoteEntry {
            id: wf_common::generate_id(),
            timestamp: note.timestamp,
            category: note.category,
            content: note.content,
            summary: note.summary,
            token_count: note.token_count,
            created_at: wf_common::time::now(),
            updated_at: wf_common::time::now(),
        };
        self.notes
            .entry(session_id.to_string())
            .or_default()
            .push(entry.clone());
        entry
    }

    fn get(&self, session_id: &str, note_id: &str) -> Option<NoteEntry> {
        self.notes
            .get(session_id)
            .and_then(|notes| notes.iter().find(|n| n.id == note_id).cloned())
    }

    fn update(&self, session_id: &str, note_id: &str, patch: NotePatch) -> Option<NoteEntry> {
        let mut notes = self.notes.get_mut(session_id)?;
        let entry = notes.iter_mut().find(|n| n.id == note_id)?;
        if let Some(v) = patch.category {
            entry.category = v;
        }
        if let Some(v) = patch.content {
            entry.content = v;
        }
        if let Some(v) = patch.summary {
            entry.summary = v;
        }
        if let Some(v) = patch.token_count {
            entry.token_count = v;
        }
        if let Some(v) = patch.timestamp {
            entry.timestamp = v;
        }
        entry.updated_at = wf_common::time::now();
        Some(entry.clone())
    }

    fn list(&self, session_id: &str, category: Option<&str>) -> Vec<NoteEntry> {
        let mut notes: Vec<NoteEntry> = self
            .notes
            .get(session_id)
            .map(|notes| {
                notes
                    .iter()
                    .filter(|n| category.map(|c| n.category == c).unwrap_or(true))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // Newest first; ids (uuid v7) break same-millisecond ties.
        notes.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        notes
    }

    fn search(&self, session_id: &str, term: &str) -> Vec<NoteEntry> {
        let term = term.to_lowercase();
        let mut notes: Vec<NoteEntry> = self
            .notes
            .get(session_id)
            .map(|notes| {
                notes
                    .iter()
                    .filter(|n| {
                        n.content.to_lowercase().contains(&term)
                            || n.summary.to_lowercase().contains(&term)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        notes.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        notes
    }

    fn delete(&self, session_id: &str, note_id: &str) -> bool {
        self.notes
            .get_mut(session_id)
            .map(|mut notes| {
                let before = notes.len();
                notes.retain(|n| n.id != note_id);
                notes.len() != before
            })
            .unwrap_or(false)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite_backend {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    /// SQLite file backend. A single connection is shared behind a mutex;
    /// schema and indexes mirror the `session_notes` table.
    pub struct SqliteNoteStore {
        conn: Mutex<rusqlite::Connection>,
    }

    impl SqliteNoteStore {
        pub fn open(db_path: &str) -> Result<Self, String> {
            if let Some(parent) = Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS session_notes (
                    id          TEXT PRIMARY KEY,
                    session_id  TEXT NOT NULL,
                    category    TEXT NOT NULL DEFAULT 'general',
                    content     TEXT NOT NULL,
                    summary     TEXT NOT NULL DEFAULT '',
                    token_count INTEGER NOT NULL DEFAULT 0,
                    timestamp   TEXT NOT NULL,
                    created_at  INTEGER NOT NULL,
                    updated_at  INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_sn_session_category
                    ON session_notes(session_id, category);
                CREATE INDEX IF NOT EXISTS idx_sn_session_created
                    ON session_notes(session_id, created_at DESC);",
            )
            .map_err(|e| e.to_string())?;
            Ok(Self {
                conn: Mutex::new(conn),
            })
        }
    }

    struct NoteRow {
        id: String,
        category: String,
        content: String,
        summary: String,
        token_count: i64,
        timestamp: String,
        created_at: i64,
        updated_at: i64,
    }

    impl From<NoteRow> for NoteEntry {
        fn from(row: NoteRow) -> Self {
            Self {
                id: row.id,
                timestamp: row.timestamp,
                category: row.category,
                content: row.content,
                summary: row.summary,
                token_count: row.token_count,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }
    }

    impl SqliteNoteStore {
        fn query_entries(
            conn: &rusqlite::Connection,
            sql: &str,
            params: &[&dyn rusqlite::types::ToSql],
        ) -> Vec<NoteEntry> {
            let mut stmt = match conn.prepare(sql) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "session-note query failed");
                    return Vec::new();
                }
            };
            let rows = stmt
                .query_map(params, |row| {
                    Ok(NoteRow {
                        id: row.get("id")?,
                        category: row.get("category")?,
                        content: row.get("content")?,
                        summary: row.get("summary")?,
                        token_count: row.get("token_count")?,
                        timestamp: row.get("timestamp")?,
                        created_at: row.get("created_at")?,
                        updated_at: row.get("updated_at")?,
                    })
                })
                .map(|it| it.flatten().map(NoteEntry::from).collect())
                .unwrap_or_default();
            rows
        }

        fn fetch_one(&self, session_id: &str, note_id: &str) -> Option<NoteEntry> {
            let conn = self.conn.lock().ok()?;
            Self::query_entries(
                &conn,
                "SELECT * FROM session_notes WHERE session_id = ?1 AND id = ?2",
                &[&session_id, &note_id],
            )
            .into_iter()
            .next()
        }
    }

    impl SessionNoteStore for SqliteNoteStore {
        fn save(&self, session_id: &str, note: NewNote) -> NoteEntry {
            let entry = NoteEntry {
                id: wf_common::generate_id(),
                timestamp: note.timestamp,
                category: note.category,
                content: note.content,
                summary: note.summary,
                token_count: note.token_count,
                created_at: wf_common::time::now(),
                updated_at: wf_common::time::now(),
            };
            let conn = self.conn.lock().expect("note store mutex poisoned");
            let _ = conn.execute(
                "INSERT INTO session_notes
                    (id, session_id, category, content, summary, token_count, timestamp, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    entry.id,
                    session_id,
                    entry.category,
                    entry.content,
                    entry.summary,
                    entry.token_count,
                    entry.timestamp,
                    entry.created_at,
                    entry.updated_at
                ],
            );
            entry
        }

        fn get(&self, session_id: &str, note_id: &str) -> Option<NoteEntry> {
            self.fetch_one(session_id, note_id)
        }

        fn update(&self, session_id: &str, note_id: &str, patch: NotePatch) -> Option<NoteEntry> {
            {
                let conn = self.conn.lock().expect("note store mutex poisoned");
                let updated_at = wf_common::time::now();
                if patch.category.is_some() {
                    let _ = conn.execute(
                        "UPDATE session_notes SET category = ?1, updated_at = ?2
                         WHERE session_id = ?3 AND id = ?4",
                        rusqlite::params![
                            patch.category.as_deref().unwrap_or(""),
                            updated_at,
                            session_id,
                            note_id
                        ],
                    );
                }
                if patch.content.is_some() {
                    let _ = conn.execute(
                        "UPDATE session_notes SET content = ?1, updated_at = ?2
                         WHERE session_id = ?3 AND id = ?4",
                        rusqlite::params![
                            patch.content.as_deref().unwrap_or(""),
                            updated_at,
                            session_id,
                            note_id
                        ],
                    );
                }
                if patch.summary.is_some() {
                    let _ = conn.execute(
                        "UPDATE session_notes SET summary = ?1, updated_at = ?2
                         WHERE session_id = ?3 AND id = ?4",
                        rusqlite::params![
                            patch.summary.as_deref().unwrap_or(""),
                            updated_at,
                            session_id,
                            note_id
                        ],
                    );
                }
                if patch.token_count.is_some() {
                    let _ = conn.execute(
                        "UPDATE session_notes SET token_count = ?1, updated_at = ?2
                         WHERE session_id = ?3 AND id = ?4",
                        rusqlite::params![
                            patch.token_count.unwrap_or(0),
                            updated_at,
                            session_id,
                            note_id
                        ],
                    );
                }
                if patch.timestamp.is_some() {
                    let _ = conn.execute(
                        "UPDATE session_notes SET timestamp = ?1, updated_at = ?2
                         WHERE session_id = ?3 AND id = ?4",
                        rusqlite::params![
                            patch.timestamp.as_deref().unwrap_or(""),
                            updated_at,
                            session_id,
                            note_id
                        ],
                    );
                }
            }
            self.fetch_one(session_id, note_id)
        }

        fn list(&self, session_id: &str, category: Option<&str>) -> Vec<NoteEntry> {
            let conn = self.conn.lock().expect("note store mutex poisoned");
            match category {
                Some(category) => Self::query_entries(
                    &conn,
                    "SELECT * FROM session_notes WHERE session_id = ?1 AND category = ?2
                     ORDER BY created_at DESC, id DESC",
                    &[&session_id, &category],
                ),
                None => Self::query_entries(
                    &conn,
                    "SELECT * FROM session_notes WHERE session_id = ?1
                     ORDER BY created_at DESC, id DESC",
                    &[&session_id],
                ),
            }
        }

        fn search(&self, session_id: &str, term: &str) -> Vec<NoteEntry> {
            let pattern = format!("%{}%", term.to_lowercase());
            let conn = self.conn.lock().expect("note store mutex poisoned");
            Self::query_entries(
                &conn,
                "SELECT * FROM session_notes
                 WHERE session_id = ?1
                   AND (LOWER(content) LIKE ?2 OR LOWER(summary) LIKE ?2)
                 ORDER BY created_at DESC, id DESC",
                &[&session_id, &pattern],
            )
        }

        fn delete(&self, session_id: &str, note_id: &str) -> bool {
            let conn = self.conn.lock().expect("note store mutex poisoned");
            conn.execute(
                "DELETE FROM session_notes WHERE session_id = ?1 AND id = ?2",
                rusqlite::params![session_id, note_id],
            )
            .map(|changed| changed > 0)
            .unwrap_or(false)
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite_backend::SqliteNoteStore;

#[cfg(test)]
mod tests {
    use super::*;

    fn new_note(content: &str, category: &str) -> NewNote {
        NewNote {
            category: category.to_string(),
            content: content.to_string(),
            summary: "".to_string(),
            token_count: 4,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn store_backends() -> Vec<(String, Arc<dyn SessionNoteStore>)> {
        let mut backends: Vec<(String, Arc<dyn SessionNoteStore>)> =
            vec![("memory".to_string(), Arc::new(InMemoryNoteStore::new()))];
        #[cfg(feature = "sqlite")]
        {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("notes.db");
            let store = SqliteNoteStore::open(path.to_str().unwrap()).expect("sqlite store");
            backends.push(("sqlite".to_string(), Arc::new(store)));
            std::mem::forget(dir); // keep the db file alive for the test
        }
        backends
    }

    async fn sleep_ms(ms: u64) {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    /// Seed three notes in distinct categories with increasing creation
    /// timestamps, returning the ids in creation order.
    async fn seed(store: &dyn SessionNoteStore) -> Vec<String> {
        let mut ids = Vec::new();
        for (content, category) in [
            ("alpha content", "work"),
            ("beta content", "personal"),
            ("gamma content", "work"),
        ] {
            ids.push(store.save("sess-1", new_note(content, category)).id);
            sleep_ms(3).await;
        }
        ids
    }

    #[tokio::test]
    async fn crud_category_filter_and_search() {
        for (name, store) in store_backends() {
            let ids = seed(store.as_ref()).await;

            // list: newest first
            let all = store.list("sess-1", None);
            assert_eq!(
                all.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
                vec![ids[2].as_str(), ids[1].as_str(), ids[0].as_str()],
                "{name}: list must be newest first"
            );
            assert_eq!(all[0].content, "gamma content");

            // list: category filter
            let work = store.list("sess-1", Some("work"));
            assert_eq!(work.len(), 2, "{name}: category filter");
            assert!(work.iter().all(|n| n.category == "work"));

            // get: found + missing
            let got = store.get("sess-1", &ids[0]).expect("{name}: note exists");
            assert_eq!(got.content, "alpha content");
            assert!(store.get("sess-1", "no-such-id").is_none());

            // update: partial patch keeps untouched fields
            let updated = store
                .update(
                    "sess-1",
                    &ids[0],
                    NotePatch {
                        category: Some("research".to_string()),
                        content: Some("alpha v2".to_string()),
                        summary: None,
                        token_count: None,
                        timestamp: None,
                    },
                )
                .expect("{name}: updated");
            assert_eq!(updated.category, "research");
            assert_eq!(updated.content, "alpha v2");
            assert_eq!(updated.summary, "");
            assert!(updated.updated_at >= updated.created_at);
            assert!(store
                .update(
                    "sess-1",
                    "no-such-id",
                    NotePatch {
                        category: None,
                        content: None,
                        summary: None,
                        token_count: None,
                        timestamp: None,
                    }
                )
                .is_none());

            // search: case-insensitive substring over content
            let hits = store.search("sess-1", "ALPHA");
            assert_eq!(hits.len(), 1, "{name}: search");
            assert_eq!(hits[0].id, ids[0]);

            // delete: idempotent
            assert!(store.delete("sess-1", &ids[1]), "{name}: delete");
            assert!(!store.delete("sess-1", &ids[1]), "{name}: delete twice");
            assert_eq!(store.list("sess-1", None).len(), 2, "{name}: after delete");
        }
    }

    #[tokio::test]
    async fn sessions_are_isolated() {
        for (name, store) in store_backends() {
            store.save("sess-a", new_note("only a", "general"));
            store.save("sess-b", new_note("only b", "general"));
            let a = store.list("sess-a", None);
            assert_eq!(a.len(), 1, "{name}: session isolation");
            assert_eq!(a[0].content, "only a");
            assert_eq!(store.list("sess-b", None).len(), 1);
        }
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_persistence_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notes.db");
        let path_str = path.to_str().unwrap().to_string();

        {
            let store = SqliteNoteStore::open(&path_str).expect("sqlite store");
            let id = store.save("sess-1", new_note("persisted note", "work")).id;
            assert!(store.delete("sess-1", &id)); // delete one to prove writes land
            store.save("sess-1", new_note("kept note", "personal"));
        }

        // A fresh store over the same file must see the surviving row.
        let reopened = SqliteNoteStore::open(&path_str).expect("reopened store");
        let notes = reopened.list("sess-1", None);
        assert_eq!(notes.len(), 1, "sqlite data must survive reopen");
        assert_eq!(notes[0].content, "kept note");
        assert_eq!(notes[0].category, "personal");
    }
}
