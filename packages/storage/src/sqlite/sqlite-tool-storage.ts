/**
 * SQLite Tool Storage - REFACTORED
 */

import type { ToolStorageMetadata } from "@wf-agent/types";
import type { ToolStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteToolStorage
  extends SqliteKeyValueStorageBase<ToolStorageMetadata>
  implements ToolStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<ToolStorageMetadata> {
    return {
      tableName: "tools",
      blobTableName: "tools_blob",

      rowToMetadata: (row) => ({
        toolId: row.tool_id,
        type: row.type,
        description: row.description,
        category: row.category,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
      }),

      metadataToValues: (meta) => ({
        tool_id: meta.toolId,
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
      CREATE TABLE IF NOT EXISTS tools (
        id TEXT PRIMARY KEY,
        tool_id TEXT NOT NULL UNIQUE,
        type TEXT NOT NULL,
        description TEXT,
        category TEXT,
        tags TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS tools_blob (
        tools_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (tools_id) REFERENCES tools(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_tools_type ON tools(type);
      CREATE INDEX IF NOT EXISTS idx_tools_tool_id ON tools(tool_id);
    `);
  }
}
