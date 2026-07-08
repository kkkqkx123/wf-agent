/**
 * PostgreSQL Node Template Storage - REFACTORED
 */

import type { NodeTemplateStorageMetadata } from "@wf-agent/types";
import type { NodeTemplateStorageAdapter } from "../types/adapter/node-template-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresNodeTemplateStorage
  extends PostgresKeyValueStorageBase<NodeTemplateStorageMetadata>
  implements NodeTemplateStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
        tags: row.tags,
        createdAt: row.created_at?.getTime(),
        updatedAt: row.updated_at?.getTime(),
      }),

      metadataToValues: (meta) => ({
        name: meta.name,
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
        CREATE TABLE IF NOT EXISTS node_templates (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          type TEXT NOT NULL,
          description TEXT,
          category TEXT,
          tags JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS node_templates_blob (
          node_templates_id TEXT PRIMARY KEY REFERENCES node_templates(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_node_templates_name ON node_templates(name);
        CREATE INDEX IF NOT EXISTS idx_node_templates_type ON node_templates(type);
      `);
    } finally {
      client.release();
    }
  }
}
