/**
 * Storage Configuration Processor
 *
 * Provides functions for processing and merging storage configuration.
 * This module handles the business logic for storage config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in api/shared/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type {
  StorageConfig,
  JsonStorageConfig,
  SqliteStorageConfig,
  PostgresStorageConfig,
} from "@wf-agent/types";

/**
 * Default JSON storage configuration
 */
const DEFAULT_JSON_STORAGE: JsonStorageConfig = {
  baseDir: "./storage",
  enableFileLock: true,
  compression: {
    enabled: true,
    algorithm: "gzip",
    threshold: 1024,
  },
};

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
  type: "json",
  json: DEFAULT_JSON_STORAGE,
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

  const merged: StorageConfig = {
    type,
    json: userConfig.json
      ? deepMergeJsonStorage(DEFAULT_JSON_STORAGE, userConfig.json)
      : DEFAULT_JSON_STORAGE,
  };

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
 * Deep merge JSON storage config (handles nested compression object)
 */
function deepMergeJsonStorage(
  defaults: JsonStorageConfig,
  user: Partial<JsonStorageConfig>,
): JsonStorageConfig {
  return {
    ...defaults,
    ...user,
    compression: user.compression
      ? { ...defaults.compression!, ...user.compression }
      : defaults.compression,
  };
}

/**
 * Get environment-specific defaults for storage.
 */
export function getStorageEnvironmentDefaults(env: "development" | "production"): StorageConfig {
  if (env === "development") {
    return {
      type: "json",
      json: {
        ...DEFAULT_JSON_STORAGE,
        baseDir: "./dev-storage",
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
