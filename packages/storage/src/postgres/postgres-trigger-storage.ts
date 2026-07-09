/**
 * PostgreSQL Trigger Storage - REFACTORED
 */

import type { TriggerStorageMetadata } from "@wf-agent/types";
import type { TriggerStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresTriggerStorage
  extends PostgresKeyValueStorageBase<TriggerStorageMetadata>
  implements TriggerStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
        tags: row.tags,
        enabled: row.enabled,
        createdAt: row.created_at?.getTime(),
        updatedAt: row.updated_at?.getTime(),
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        description: meta.description ?? null,
        category: meta.category ?? null,
        tags: meta.tags ? meta.tags : null,
        enabled: meta.enabled ?? true,
      }),
    };
  }

  protected async createPostgresSchema(): Promise<void> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      await client.query(`
        CREATE TABLE IF NOT EXISTS triggers (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          description TEXT,
          category TEXT,
          enabled BOOLEAN DEFAULT TRUE,
          tags JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS triggers_blob (
          triggers_id TEXT PRIMARY KEY REFERENCES triggers(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_triggers_name ON triggers(name);
        CREATE INDEX IF NOT EXISTS idx_triggers_enabled ON triggers(enabled);
      `);
    } finally {
      client.release();
    }
  }
}
