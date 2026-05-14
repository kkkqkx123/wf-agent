/**
 * Storage Configuration Types
 * Unified storage configuration types for all applications
 */

/**
 * Compression Algorithm
 */
export type CompressionAlgorithm = "gzip" | "brotli";

/**
 * Compression Configuration
 */
export interface CompressionConfig {
  enabled: boolean;
  algorithm: CompressionAlgorithm;
  threshold: number;
}

/**
 * JSON Storage Configuration
 */
export interface JsonStorageConfig {
  baseDir: string;
  enableFileLock: boolean;
  compression?: CompressionConfig;
}

/**
 * SQLite Storage Configuration
 */
export interface SqliteStorageConfig {
  dbPath: string;
  enableWAL: boolean;
  enableLogging: boolean;
  readonly: boolean;
  fileMustExist: boolean;
  timeout: number;
}

/**
 * PostgreSQL Storage Configuration
 */
export interface PostgresStorageConfig {
  /** Database host */
  host: string;
  /** Database port (default: 5432) */
  port?: number;
  /** Database user */
  username: string;
  /** Database password */
  password: string;
  /** Database name */
  database: string;
  /** Enable SSL connection (default: false) */
  ssl?: boolean;
  /** Maximum pool size (default: 20) */
  poolSize?: number;
  /** Minimum idle connections (default: 1) */
  minConnections?: number;
  /** Idle connection timeout in ms (default: 30000) */
  idleTimeout?: number;
  /** Connection timeout in ms (default: 5000) */
  connectionTimeout?: number;
  /** Maximum uses per connection before recycle (default: Infinity) */
  maxUses?: number;
}

/**
 * Storage Type
 */
export type StorageType = "json" | "sqlite" | "postgres" | "memory";

/**
 * Storage Configuration
 */
export interface StorageConfig {
  type: StorageType;
  json?: JsonStorageConfig;
  sqlite?: SqliteStorageConfig;
  postgres?: PostgresStorageConfig;
}
