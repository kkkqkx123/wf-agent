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
 * Storage Type
 */
export type StorageType = "json" | "sqlite" | "memory";

/**
 * Storage Configuration
 */
export interface StorageConfig {
  type: StorageType;
  json?: JsonStorageConfig;
  sqlite?: SqliteStorageConfig;
}
