/**
 * SQLite Node Template Storage - REFACTORED
 */

import type { NodeTemplateStorageMetadata } from "@wf-agent/types";
import type { NodeTemplateStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteNodeTemplateStorage
  extends SqliteKeyValueStorageBase<NodeTemplateStorageMetadata>
  implements NodeTemplateStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<NodeTemplateStorageMetadata> {
    return {
      tableName: "node_templates",
      blobTableName: "node_templates_blob",

      rowToMetadata: (row) => ({
        name: row.name,
        type: row.type,
        description: row.description,
        category: row.category,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        createdAt: row.created_at,
        updatedAt: row.updated_at,
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        type: meta.type,
        description: meta.description ?? null,
        category: meta.category ?? null,
        tags: meta.tags ? JSON.stringify(meta.tags) : null,
      }),
    };
  }

  protected createSqliteSchema(): void {
    const db = this.getDb();

    db.exec(`
      CREATE TABLE IF NOT EXISTS node_templates (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        type TEXT NOT NULL,
        description TEXT,
        category TEXT,
        tags TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS node_templates_blob (
        node_templates_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (node_templates_id) REFERENCES node_templates(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_node_templates_name ON node_templates(name);
      CREATE INDEX IF NOT EXISTS idx_node_templates_type ON node_templates(type);
    `);
  }
}
