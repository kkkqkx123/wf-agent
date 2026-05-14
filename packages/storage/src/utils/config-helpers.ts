/**
 * Configuration Helper Utilities
 * Provides utilities for converting high-level storage configurations to base storage configs
 */

import type { PostgresStorageConfig } from '@wf-agent/types';
import type { SqliteStorageConfig } from '@wf-agent/types';
import type { BasePostgresStorageConfig } from '../postgres/base-postgres-storage.js';
import type { BaseSqliteStorageConfig } from '../sqlite/base-sqlite-storage.js';

/**
 * Convert high-level PostgreSQL config to base storage config
 * @param config High-level PostgreSQL configuration
 * @returns Base PostgreSQL storage configuration ready for use
 */
export function convertToPostgresBaseConfig(
  config: PostgresStorageConfig
): BasePostgresStorageConfig {
  const connectionString = buildPostgresConnectionString(config);
  
  return {
    connectionString,
    poolConfig: {
      max: config.poolSize ?? 20,
      min: config.minConnections ?? 1,
      idleTimeoutMillis: config.idleTimeout ?? 30000,
      connectionTimeoutMillis: config.connectionTimeout ?? 5000,
      maxUses: config.maxUses,
    },
    useConnectionPool: true,
  };
}

/**
 * Build PostgreSQL connection string from components
 * @param config PostgreSQL configuration
 * @returns Formatted connection string
 */
function buildPostgresConnectionString(config: PostgresStorageConfig): string {
  const { host, port = 5432, username, password, database, ssl } = config;
  
  // Encode credentials and database name to handle special characters
  let url = `postgresql://${encodeURIComponent(username)}:${encodeURIComponent(password)}`;
  url += `@${host}:${port}/${encodeURIComponent(database)}`;
  
  // Add SSL parameter if enabled
  if (ssl) {
    url += '?sslmode=require';
  }
  
  return url;
}

/**
 * Convert high-level SQLite config to base storage config
 * @param config High-level SQLite configuration
 * @returns Base SQLite storage configuration ready for use
 */
export function convertToSqliteBaseConfig(
  config: SqliteStorageConfig
): BaseSqliteStorageConfig {
  return {
    dbPath: config.dbPath,
    enableLogging: config.enableLogging ?? false,
    readonly: config.readonly ?? false,
    fileMustExist: config.fileMustExist ?? false,
    timeout: config.timeout ?? 5000,
    useConnectionPool: true,
  };
}
