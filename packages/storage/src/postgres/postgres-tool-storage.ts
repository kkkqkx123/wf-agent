/**
 * PostgreSQL Tool Storage - REFACTORED
 */

import type { ToolStorageMetadata } from "@wf-agent/types";
import type { ToolStorageAdapter } from "../types/adapter/tool-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresToolStorage
  extends PostgresKeyValueStorageBase<ToolStorageMetadata>
  implements ToolStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
        tags: row.tags,
      }),

      metadataToValues: (meta) => ({
        tool_id: meta.toolId,
        type: meta.type,
        description: meta.description ?? null,
        category: meta.category ?? null,
        tags: meta.tags ? meta.tags : null,
      }),
    };
  }

  protected async createPostgresSchema(): Promise<void> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      await client.query(`
        CREATE TABLE IF NOT EXISTS tools (
          id TEXT PRIMARY KEY,
          tool_id TEXT NOT NULL UNIQUE,
          type TEXT NOT NULL,
          description TEXT,
          category TEXT,
          tags JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS tools_blob (
          tools_id TEXT PRIMARY KEY REFERENCES tools(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tools_type ON tools(type);
        CREATE INDEX IF NOT EXISTS idx_tools_tool_id ON tools(tool_id);
      `);
    } finally {
      client.release();
    }
  }
}
