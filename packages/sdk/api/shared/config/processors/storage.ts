/**
 * Storage Configuration Processor
 *
 * Provides functions for processing and merging storage configuration.
 * This module handles the business logic for storage config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in a../shared/core/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type {
  StorageConfig,
  SqliteStorageConfig,
  PostgresStorageConfig,
} from "@wf-agent/types";

/**
 * Default SQLite storage configuration
 */
const DEFAULT_SQLITE_STORAGE: SqliteStorageConfig = {
  dbPath: "./data/workflow.db",
  enableWAL: true,
  enableLogging: false,
  readonly: false,
  fileMustExist: false,
  timeout: 5000,
};

/**
 * Default PostgreSQL storage configuration
 */
const DEFAULT_POSTGRES_STORAGE: PostgresStorageConfig = {
  host: "localhost",
  port: 5432,
  username: "postgres",
  password: "",
  database: "wf_agent",
  ssl: false,
  poolSize: 20,
  minConnections: 1,
  idleTimeout: 30000,
  connectionTimeout: 5000,
};

/**
 * Default storage configuration
 */
const DEFAULT_STORAGE_CONFIG: StorageConfig = {
  type: "sqlite",
  sqlite: DEFAULT_SQLITE_STORAGE,
};

/**
 * Merge user config with defaults for a storage config.
 * Deep-merges the backend-specific sub-config based on the selected type.
 *
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeStorageWithDefaults(userConfig: Partial<StorageConfig>): StorageConfig {
  const type = userConfig.type ?? DEFAULT_STORAGE_CONFIG.type;

  const merged: StorageConfig = { type };

  if (type === "sqlite" || userConfig.sqlite) {
    merged.sqlite = userConfig.sqlite
      ? { ...DEFAULT_SQLITE_STORAGE, ...userConfig.sqlite }
      : DEFAULT_SQLITE_STORAGE;
  }

  if (type === "postgres" || userConfig.postgres) {
    merged.postgres = userConfig.postgres
      ? { ...DEFAULT_POSTGRES_STORAGE, ...userConfig.postgres }
      : DEFAULT_POSTGRES_STORAGE;
  }

  return merged;
}

/**
 * Get environment-specific defaults for storage.
 */
export function getStorageEnvironmentDefaults(env: "development" | "production"): StorageConfig {
  if (env === "development") {
    return {
      type: "sqlite",
      sqlite: {
        ...DEFAULT_SQLITE_STORAGE,
        dbPath: "./dev-storage/wf-agent.db",
      },
    };
  }

  // Production: use SQLite with stricter settings
  return {
    type: "sqlite",
    sqlite: {
      ...DEFAULT_SQLITE_STORAGE,
      enableWAL: true,
      fileMustExist: true,
    },
  };
}
