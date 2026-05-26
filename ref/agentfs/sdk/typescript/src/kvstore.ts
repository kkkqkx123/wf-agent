import type { DatabasePromise } from '@tursodatabase/database-common';

export class KvStore {
  private db: DatabasePromise;

  private constructor(db: DatabasePromise) {
    this.db = db;
  }

  /**
   * Create a KvStore from an existing database connection
   */
  static async fromDatabase(db: DatabasePromise): Promise<KvStore> {
    const kv = new KvStore(db);
    await kv.initialize();
    return kv;
  }

  private async initialize(): Promise<void> {
    // Create the simplified key-value store table if it doesn't exist
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS kv (
        key TEXT PRIMARY KEY,
        value JSON NOT NULL,
        created_at INTEGER DEFAULT (unixepoch()),
        updated_at INTEGER DEFAULT (unixepoch())
      )
    `);

    // Create index on created_at for potential queries
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_kv_created_at
      ON kv(created_at)
    `);
  }

  async set(key: string, value: any): Promise<void> {
    // Use prepared statement to insert or update with JSON column
    const stmt = this.db.prepare(`
      INSERT INTO kv (key, value, updated_at)
      VALUES (?, ?, unixepoch())
      ON CONFLICT(key) DO UPDATE SET
        value = excluded.value,
        updated_at = unixepoch()
    `);

    await stmt.run(key, value);
  }

  async get<T = any>(key: string): Promise<T | undefined> {
    const stmt = this.db.prepare(`SELECT value FROM kv WHERE key = ?`);
    const row = await stmt.get(key) as { value: T } | undefined;

    if (!row) {
      return undefined;
    }

    return row.value;
  }

  async list(prefix: string): Promise<{ key: string, value: any }[]> {
    const stmt = this.db.prepare(`SELECT key, value FROM kv WHERE key LIKE ? ESCAPE '\\'`);
    const escaped = prefix.replace('\\', '\\\\').replace('%', '\\%').replace('_', '\\_');
    const rows = await stmt.all(escaped + '%') as { key: string, value: any }[];
    return rows;
  }

  async delete(key: string): Promise<void> {
    const stmt = this.db.prepare(`DELETE FROM kv WHERE key = ?`);
    await stmt.run(key);
  }
}
