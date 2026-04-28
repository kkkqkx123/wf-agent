/**
 * CLI Configuration Types
 * Contains all type definitions for CLI configuration.
 */

/**
 * Compression Algorithm
 */
export type CompressionAlgorithm = "gzip" | "brotli" | "zlib";

/**
 * Log Level
 */
export type LogLevel = "error" | "warn" | "info" | "debug";

/**
 * Output Format
 */
export type OutputFormat = "json" | "table" | "plain";

/**
 * SDK Log Level
 */
export type SDKLogLevel = "silent" | "error" | "warn" | "info" | "debug";

/**
 * Storage Type
 */
export type StorageType = "json" | "sqlite" | "memory";

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
 * Storage Configuration
 */
export interface StorageConfig {
  type: StorageType;
  json?: JsonStorageConfig;
  sqlite?: SqliteStorageConfig;
}

/**
 * Output Configuration
 */
export interface OutputConfig {
  dir: string;
  logFilePattern: string;
  enableLogTerminal: boolean;
  enableSDKLogs: boolean;
  sdkLogLevel: SDKLogLevel;
}

/**
 * Context Compression Preset Configuration
 */
export interface ContextCompressionPresetConfig {
  enabled: boolean;
  prompt?: string;
  timeout?: number;
  maxTriggers?: number;
}

/**
 * Predefined Tools Preset Configuration
 */
export interface PredefinedToolsPresetConfig {
  enabled: boolean;
  allowList?: string[];
  blockList?: string[];
  config?: {
    readFile?: {
      workspaceDir?: string;
      maxFileSize?: number;
    };
    writeFile?: {
      workspaceDir?: string;
    };
    editFile?: {
      workspaceDir?: string;
    };
    bash?: {
      defaultTimeout?: number;
      maxTimeout?: number;
    };
    sessionNote?: {
      workspaceDir?: string;
      memoryFile?: string;
    };
    backgroundShell?: {
      workspaceDir?: string;
    };
  };
}

/**
 * Predefined Prompts Preset Configuration
 */
export interface PredefinedPromptsPresetConfig {
  enabled: boolean;
}

/**
 * Presets Configuration
 */
export interface PresetsConfig {
  contextCompression?: ContextCompressionPresetConfig;
  predefinedTools?: PredefinedToolsPresetConfig;
  predefinedPrompts?: PredefinedPromptsPresetConfig;
}

/**
 * Complete CLI Configuration
 */
export interface CLIConfig {
  apiUrl?: string;
  apiKey?: string;
  defaultTimeout: number;
  verbose: boolean;
  debug: boolean;
  logLevel: LogLevel;
  outputFormat: OutputFormat;
  maxConcurrentThreads: number;
  storage?: StorageConfig;
  output?: OutputConfig;
  presets?: PresetsConfig;
}
