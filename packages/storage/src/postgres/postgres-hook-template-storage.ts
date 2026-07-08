/**
 * PostgreSQL Hook Template Storage - REFACTORED
 */

import type { HookTemplateStorageMetadata } from "@wf-agent/types";
import type { HookTemplateStorageAdapter } from "../types/adapter/hook-template-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresHookTemplateStorage
  extends PostgresKeyValueStorageBase<HookTemplateStorageMetadata>
  implements HookTemplateStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
        tags: row.tags,
        createdAt: row.created_at?.getTime(),
        updatedAt: row.updated_at?.getTime(),
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
        hook_type: meta.hookType,
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
        CREATE TABLE IF NOT EXISTS hook_templates (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          hook_type TEXT NOT NULL,
          description TEXT,
          category TEXT,
          tags JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS hook_templates_blob (
          hook_templates_id TEXT PRIMARY KEY REFERENCES hook_templates(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_hook_templates_name ON hook_templates(name);
        CREATE INDEX IF NOT EXISTS idx_hook_templates_type ON hook_templates(hook_type);
      `);
    } finally {
      client.release();
    }
  }
}
