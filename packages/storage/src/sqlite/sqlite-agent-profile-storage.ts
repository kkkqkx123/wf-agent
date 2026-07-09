/**
 * SQLite Agent Profile Storage - REFACTORED
 */

import type { AgentProfileStorageMetadata, AgentProfileListOptions } from "@wf-agent/types";
import type { AgentProfileStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { SqliteKeyValueStorageBase } from "../types/adapter/sqlite-key-value-storage-base.js";
import type { KeyValueStorageConfig } from "../types/adapter/key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "./base-sqlite-storage.js";

export class SqliteAgentProfileStorage
  extends SqliteKeyValueStorageBase<AgentProfileStorageMetadata, AgentProfileListOptions>
  implements AgentProfileStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
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

  protected createSqliteSchema(): void {
    const db = this.getDb();

    db.exec(`
      CREATE TABLE IF NOT EXISTS agent_profiles (
        id TEXT PRIMARY KEY,
        profile_id TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        description TEXT,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
      );

      CREATE TABLE IF NOT EXISTS agent_profiles_blob (
        agent_profiles_id TEXT PRIMARY KEY,
        content BLOB NOT NULL,
        FOREIGN KEY (agent_profiles_id) REFERENCES agent_profiles(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_agent_profiles_profile_id ON agent_profiles(profile_id);
      CREATE INDEX IF NOT EXISTS idx_agent_profiles_name ON agent_profiles(name);
    `);
  }
}
