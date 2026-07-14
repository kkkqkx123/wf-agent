/**
 * SQLite Note Storage for session notes
 *
 * Provides ACID-compliant, concurrent-safe storage for session notes
 * with native SQL querying and size-limit enforcement.
 */

import Database from "better-sqlite3";
import { configurePragmas, type PragmaConfig } from "./sqlite-pragma.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-note-storage");

/**
 * A single note entry row stored in the database.
 */
export interface NoteRow {
  id: string;
  session_id: string;
  category: string;
  content: string;
  summary: string;
  token_count: number;
  timestamp: string;
  created_at: number;
  updated_at: number;
}

/**
 * Category summary with aggregated statistics.
 */
export interface NoteCategorySummary {
  category: string;
  count: number;
  total_tokens: number;
}

/**
 * Session-level statistics.
 */
export interface SessionNoteStats {
  total_notes: number;
  total_tokens: number;
  categories: NoteCategorySummary[];
}

/**
 * A note entry returned to the caller (snake_case -> camelCase).
 */
export interface NoteEntryResult {
  id: string;
  timestamp: string;
  category: string;
  content: string;
  summary: string;
  tokenCount: number;
  createdAt: number;
  updatedAt: number;
}

export interface SqliteNoteStorageConfig {
  /** Path to the SQLite database file */
  dbPath: string;
  /** Maximum number of notes per session (0 = unlimited, default 1000) */
  maxNotesPerSession?: number;
  /** SQLite PRAGMA overrides */
  pragma?: PragmaConfig;
}

export class SqliteNoteStorage {
  private db: Database.Database | null = null;
  private config: SqliteNoteStorageConfig;
  constructor(config: SqliteNoteStorageConfig) {
    this.config = {
      maxNotesPerSession: 1000,
      ...config,
    };
  }

  /**
   * Open the database connection and create the schema.
   */
  initialize(): void {
    if (this.db) return;

    this.db = new Database(this.config.dbPath);

    configurePragmas(this.db, this.config.pragma);

    this.createSchema();
    logger.info("SQLite note storage initialized", { dbPath: this.config.dbPath });
  }

  /**
   * Close the database connection.
   */
  close(): void {
    if (this.db) {
      this.db.close();
      this.db = null;
      logger.info("SQLite note storage closed");
    }
  }

  /**
   * Save a note. Returns the full saved NoteEntryResult with a generated id.
   * Enforces maxNotesPerSession by trimming oldest notes when the limit is exceeded.
   */
  saveNote(
    sessionId: string,
    note: {
      category: string;
      content: string;
      summary: string;
      tokenCount: number;
      timestamp: string;
    },
  ): NoteEntryResult {
    this.ensureInitialized();

    const id = crypto.randomUUID();
    const now = Math.floor(Date.now() / 1000);

    const db = this.db!;

    const insertTransaction = db.transaction(() => {
      // Enforce max notes limit (only for new inserts, not upserts)
      const maxNotes = this.config.maxNotesPerSession!;
      if (maxNotes > 0) {
        const count = (
          db
            .prepare("SELECT COUNT(*) AS cnt FROM session_notes WHERE session_id = ?")
            .get(sessionId) as { cnt: number }
        ).cnt;

        if (count >= maxNotes) {
          // Delete the oldest notes to make room
          const overflow = count - maxNotes + 1;
          db.prepare(
            `DELETE FROM session_notes WHERE id IN (
              SELECT id FROM session_notes WHERE session_id = ?
              ORDER BY created_at ASC, id ASC LIMIT ?
            )`,
          ).run(sessionId, overflow);
        }
      }

      db.prepare(
        `INSERT INTO session_notes (id, session_id, category, content, summary, token_count, timestamp, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ).run(id, sessionId, note.category, note.content, note.summary, note.tokenCount, note.timestamp, now, now);
    });

    insertTransaction();

    return {
      id,
      timestamp: note.timestamp,
      category: note.category,
      content: note.content,
      summary: note.summary,
      tokenCount: note.tokenCount,
      createdAt: now,
      updatedAt: now,
    };
  }

  /**
   * Get a single note by session and note id.
   */
  getNote(sessionId: string, noteId: string): NoteEntryResult | null {
    this.ensureInitialized();
    const row = this.db!
      .prepare("SELECT * FROM session_notes WHERE session_id = ? AND id = ?")
      .get(sessionId, noteId) as NoteRow | undefined;

    if (!row) return null;
    return this.rowToEntry(row);
  }

  /**
   * Update an existing note. Returns the updated NoteEntryResult, or null if the note does not exist.
   * Only the provided fields are updated; omitted fields retain their original values.
   */
  updateNote(
    sessionId: string,
    noteId: string,
    note: {
      category?: string;
      content?: string;
      summary?: string;
      tokenCount?: number;
      timestamp?: string;
    },
  ): NoteEntryResult | null {
    this.ensureInitialized();

    const now = Math.floor(Date.now() / 1000);
    const db = this.db!;

    const sets: string[] = ["updated_at = ?"];
    const params: unknown[] = [now];

    if (note.category !== undefined) {
      sets.push("category = ?");
      params.push(note.category);
    }
    if (note.content !== undefined) {
      sets.push("content = ?");
      params.push(note.content);
    }
    if (note.summary !== undefined) {
      sets.push("summary = ?");
      params.push(note.summary);
    }
    if (note.tokenCount !== undefined) {
      sets.push("token_count = ?");
      params.push(note.tokenCount);
    }
    if (note.timestamp !== undefined) {
      sets.push("timestamp = ?");
      params.push(note.timestamp);
    }

    params.push(sessionId, noteId);

    const result = db
      .prepare(
        `UPDATE session_notes SET ${sets.join(", ")} WHERE session_id = ? AND id = ?`,
      )
      .run(...params);

    if (result.changes === 0) return null;

    return this.getNote(sessionId, noteId);
  }

  /**
   * List notes for a session, optionally filtered by category.
   * Ordered by created_at descending (newest first) by default.
   * Use sortBy 'updatedAt' to order by last updated time.
   */
  listNotes(
    sessionId: string,
    options?: { category?: string; limit?: number; offset?: number; sortBy?: "createdAt" | "updatedAt" },
  ): NoteEntryResult[] {
    this.ensureInitialized();

    let sql = "SELECT * FROM session_notes WHERE session_id = ?";
    const params: unknown[] = [sessionId];

    if (options?.category) {
      sql += " AND category = ?";
      params.push(options.category);
    }

    const sortColumn = options?.sortBy === "updatedAt" ? "updated_at" : "created_at";
    sql += ` ORDER BY ${sortColumn} DESC`;

    if (options?.limit) {
      sql += " LIMIT ?";
      params.push(options.limit);
    }
    if (options?.offset) {
      sql += " OFFSET ?";
      params.push(options.offset);
    }

    const rows = this.db!.prepare(sql).all(...params) as NoteRow[];
    return rows.map((r) => this.rowToEntry(r));
  }

  /**
   * List all distinct categories with note count and total tokens for a session.
   */
  listCategories(sessionId: string): NoteCategorySummary[] {
    this.ensureInitialized();
    const rows = this.db!
      .prepare(
        `SELECT category, COUNT(*) AS count, SUM(token_count) AS total_tokens
         FROM session_notes
         WHERE session_id = ?
         GROUP BY category
         ORDER BY count DESC`,
      )
      .all(sessionId) as NoteCategorySummary[];

    return rows.map((r) => ({
      category: r.category,
      count: r.count,
      total_tokens: r.total_tokens,
    }));
  }

  /**
   * Get aggregated statistics for a session.
   */
  getStats(sessionId: string): SessionNoteStats {
    this.ensureInitialized();

    const total = this.db!
      .prepare(
        `SELECT COUNT(*) AS total_notes, COALESCE(SUM(token_count), 0) AS total_tokens
         FROM session_notes WHERE session_id = ?`,
      )
      .get(sessionId) as { total_notes: number; total_tokens: number };

    return {
      total_notes: total.total_notes,
      total_tokens: total.total_tokens,
      categories: this.listCategories(sessionId),
    };
  }

  /**
   * Delete a single note.
   */
  deleteNote(sessionId: string, noteId: string): boolean {
    this.ensureInitialized();
    const result = this.db!
      .prepare("DELETE FROM session_notes WHERE session_id = ? AND id = ?")
      .run(sessionId, noteId);
    return result.changes > 0;
  }

  /**
   * Delete all notes for a session.
   */
  clearSession(sessionId: string): void {
    this.ensureInitialized();
    this.db!.prepare("DELETE FROM session_notes WHERE session_id = ?").run(sessionId);
  }

  private ensureInitialized(): void {
    if (!this.db) {
      throw new Error(
        "SqliteNoteStorage not initialized. Call initialize() before any operation.",
      );
    }
  }

  private createSchema(): void {
    this.db!.exec(`
      CREATE TABLE IF NOT EXISTS session_notes (
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
        ON session_notes(session_id, created_at DESC);

      CREATE INDEX IF NOT EXISTS idx_sn_session_updated
        ON session_notes(session_id, updated_at DESC);
    `);
  }

  private rowToEntry(row: NoteRow): NoteEntryResult {
    return {
      id: row.id,
      timestamp: row.timestamp,
      category: row.category,
      content: row.content,
      summary: row.summary,
      tokenCount: row.token_count,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  }
}