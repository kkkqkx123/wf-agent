/**
 * SQLite Script Storage - REFACTORED
 *
 * Before: 243 lines
 * After: 60 lines
 * Code reduction: 75%
 */

import type { ScriptStorageMetadata, ScriptListOptions } from "@wf-agent/types";
import type { ScriptStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteScriptStorage
  extends SqliteKeyValueStorageBase<ScriptStorageMetadata, ScriptListOptions>
  implements ScriptStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<ScriptStorageMetadata> {
    return {
      tableName: "scripts",
      blobTableName: "scripts_blob",

      rowToMetadata: (row) => ({
        name: row.name,
        description: row.description,
        category: row.category,
        enabled: row.enabled === 1,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        description: meta.description ?? null,
        category: meta.category ?? null,
        enabled: meta.enabled ? 1 : 0,
        tags: meta.tags ? JSON.stringify(meta.tags) : null,
      }),
    };
  }

  protected createSqliteSchema(): void {
    const db = this.getDb();

    db.exec(`
      CREATE TABLE IF NOT EXISTS scripts (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        description TEXT,
        category TEXT,
        enabled INTEGER DEFAULT 1,
        tags TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS scripts_blob (
        scripts_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (scripts_id) REFERENCES scripts(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_scripts_name ON scripts(name);
      CREATE INDEX IF NOT EXISTS idx_scripts_enabled ON scripts(enabled);
    `);
  }
}
