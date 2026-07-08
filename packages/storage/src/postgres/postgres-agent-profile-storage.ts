/**
 * PostgreSQL Agent Profile Storage - REFACTORED
 */

import type { AgentProfileStorageMetadata } from "@wf-agent/types";
import type { AgentProfileStorageAdapter } from "../types/adapter/agent-profile-adapter.js";
import { PostgresKeyValueStorageBase } from "../types/adapter/postgres-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "./base-postgres-storage.js";

export class PostgresAgentProfileStorage
  extends PostgresKeyValueStorageBase<AgentProfileStorageMetadata>
  implements AgentProfileStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  protected getConfig(): KeyValueStorageConfig<AgentProfileStorageMetadata> {
    return {
      tableName: "agent_profiles",
      blobTableName: "agent_profiles_blob",

      rowToMetadata: (row) => ({
        profileId: row.profile_id,
        name: row.name,
        description: row.description,
      }),

      metadataToValues: (meta) => ({
        profile_id: meta.profileId,
        name: meta.name,
        description: meta.description ?? null,
      }),
    };
  }

  protected async createPostgresSchema(): Promise<void> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      await client.query(`
        CREATE TABLE IF NOT EXISTS agent_profiles (
          id TEXT PRIMARY KEY,
          profile_id TEXT NOT NULL UNIQUE,
          name TEXT NOT NULL,
          description TEXT,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS agent_profiles_blob (
          agent_profiles_id TEXT PRIMARY KEY REFERENCES agent_profiles(id) ON DELETE CASCADE,
          content BYTEA NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_agent_profiles_profile_id ON agent_profiles(profile_id);
        CREATE INDEX IF NOT EXISTS idx_agent_profiles_name ON agent_profiles(name);
      `);
    } finally {
      client.release();
    }
  }
}
