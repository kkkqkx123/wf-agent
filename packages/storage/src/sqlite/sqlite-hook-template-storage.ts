/**
 * SQLite Hook Template Storage - REFACTORED
 */

import type { HookTemplateStorageMetadata, HookTemplateListOptions } from "@wf-agent/types";
import type { HookTemplateStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteHookTemplateStorage
  extends SqliteKeyValueStorageBase<HookTemplateStorageMetadata, HookTemplateListOptions>
  implements HookTemplateStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<HookTemplateStorageMetadata> {
    return {
      tableName: "hook_templates",
      blobTableName: "hook_templates_blob",

      rowToMetadata: (row) => ({
        name: row.name,
        hookType: row.hook_type,
        description: row.description,
        category: row.category,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        createdAt: row.created_at,
        updatedAt: row.updated_at,
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        hook_type: meta.hookType,
        description: meta.description ?? null,
        category: meta.category ?? null,
        tags: meta.tags ? JSON.stringify(meta.tags) : null,
      }),
    };
  }

  protected createSqliteSchema(): void {
    const db = this.getDb();

    db.exec(`
      CREATE TABLE IF NOT EXISTS hook_templates (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        hook_type TEXT NOT NULL,
        description TEXT,
        category TEXT,
        tags TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS hook_templates_blob (
        hook_templates_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (hook_templates_id) REFERENCES hook_templates(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_hook_templates_name ON hook_templates(name);
      CREATE INDEX IF NOT EXISTS idx_hook_templates_type ON hook_templates(hook_type);
    `);
  }
}
