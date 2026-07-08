/**
 * SQLite Trigger Storage - REFACTORED
 */

import type { TriggerStorageMetadata } from "@wf-agent/types";
import type { TriggerStorageAdapter } from "../types/adapter/trigger-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteTriggerStorage
  extends SqliteKeyValueStorageBase<TriggerStorageMetadata>
  implements TriggerStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<TriggerStorageMetadata> {
    return {
      tableName: "triggers",
      blobTableName: "triggers_blob",

      rowToMetadata: (row) => ({
        name: row.name,
        description: row.description,
        category: row.category,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        enabled: row.enabled === 1,
        createdAt: row.created_at,
        updatedAt: row.updated_at,
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        description: meta.description ?? null,
        category: meta.category ?? null,
        tags: meta.tags ? JSON.stringify(meta.tags) : null,
        enabled: meta.enabled ? 1 : 0,
      }),
    };
  }

  protected createSqliteSchema(): void {
    const db = this.getDb();

    db.exec(`
      CREATE TABLE IF NOT EXISTS triggers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        description TEXT,
        category TEXT,
        enabled INTEGER DEFAULT 1,
        tags TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS triggers_blob (
        triggers_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (triggers_id) REFERENCES triggers(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_triggers_name ON triggers(name);
      CREATE INDEX IF NOT EXISTS idx_triggers_enabled ON triggers(enabled);
    `);
  }
}
