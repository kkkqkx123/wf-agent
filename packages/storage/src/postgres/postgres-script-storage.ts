/**
 * PostgreSQL Script Storage - REFACTORED
 */

import type { ScriptStorageMetadata } from "@wf-agent/types";
import type { ScriptStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresScriptStorage
  extends PostgresKeyValueStorageBase<ScriptStorageMetadata>
  implements ScriptStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
        enabled: row.enabled,
        tags: row.tags,
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        description: meta.description ?? null,
        category: meta.category ?? null,
        enabled: meta.enabled ?? true,
        tags: meta.tags ? meta.tags : null,
      }),
    };
  }

  protected async createPostgresSchema(): Promise<void> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      await client.query(`
        CREATE TABLE IF NOT EXISTS scripts (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          description TEXT,
          category TEXT,
          enabled BOOLEAN DEFAULT TRUE,
          tags JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS scripts_blob (
          scripts_id TEXT PRIMARY KEY REFERENCES scripts(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_scripts_name ON scripts(name);
        CREATE INDEX IF NOT EXISTS idx_scripts_enabled ON scripts(enabled);
      `);
    } finally {
      client.release();
    }
  }
}
