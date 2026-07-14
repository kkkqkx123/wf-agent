/**
 * Zod Schemas for Configuration Validation
 * Provides runtime validation schemas synchronized with TypeScript type definitions
 */

import { z } from "zod";

// Re-export metrics and timeout schemas
export {
  MetricsConfigSchema,
  MetricCollectorConfigSchema,
  isMetricsConfig,
  isMetricCollectorConfig,
} from "./metrics-schema.js";

export {
  TimeoutConfigSchema,
  isTimeoutConfig,
} from "./timeout-schema.js";

// ============================================================================
// Storage Configuration Schemas
// ============================================================================

/**
 * Compression Algorithm Schema
 */
export const CompressionAlgorithmSchema = z.enum(["gzip", "brotli"]);

/**
 * Compression Configuration Schema
 */
export const CompressionConfigSchema = z.object({
  enabled: z.boolean(),
  algorithm: CompressionAlgorithmSchema,
  threshold: z.number().positive(),
});

/**
 * SQLite Storage Configuration Schema
 */
export const SqliteStorageConfigSchema = z.object({
  dbPath: z.string().min(1, "Database path is required"),
  enableWAL: z.boolean(),
  enableLogging: z.boolean(),
  readonly: z.boolean(),
  fileMustExist: z.boolean(),
  timeout: z.number().positive(),
});

/**
 * Storage Type Schema
 */
export const StorageTypeSchema = z.enum(["sqlite", "memory", "postgres"]);

/**
 * Storage Configuration Schema
 */
export const StorageConfigSchema = z.object({
  type: StorageTypeSchema,
  sqlite: SqliteStorageConfigSchema.optional(),
  postgres: z.object({
    host: z.string(),
    port: z.number().optional(),
    username: z.string(),
    password: z.string(),
    database: z.string(),
    ssl: z.boolean().optional(),
    poolSize: z.number().positive().optional(),
    minConnections: z.number().positive().optional(),
    idleTimeout: z.number().positive().optional(),
    connectionTimeout: z.number().positive().optional(),
    maxUses: z.number().positive().optional(),
  }).optional(),
});

// ============================================================================
// Preset Configuration Schemas
// ============================================================================

/**
 * Context Compression Preset Configuration Schema
 */
export const ContextCompressionPresetConfigSchema = z.object({
  enabled: z.boolean(),
  prompt: z.string().optional(),
  timeout: z.number().positive().optional(),
  maxTriggers: z.number().positive().optional(),
});

/**
 * Predefined Tools Preset Configuration Schema
 */
export const PredefinedToolsPresetConfigSchema = z.object({
  enabled: z.boolean(),
  allowList: z.array(z.string()).optional(),
  blockList: z.array(z.string()).optional(),
  config: z
    .object({
      readFile: z
        .object({
          workspaceDir: z.string().optional(),
          maxFileSize: z.number().positive().optional(),
        })
        .optional(),
      writeFile: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
      editFile: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
      bash: z
        .object({
          defaultTimeout: z.number().positive().optional(),
          maxTimeout: z.number().positive().optional(),
        })
        .optional(),
      sessionNote: z
        .object({
          workspaceDir: z.string().optional(),
          memoryFile: z.string().optional(),
        })
        .optional(),
      backgroundShell: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
    })
    .optional(),
});

/**
 * Predefined Prompts Preset Configuration Schema
 */
export const PredefinedPromptsPresetConfigSchema = z.object({
  enabled: z.boolean(),
});

/**
 * Presets Configuration Schema
 */
export const PresetsConfigSchema = z.object({
  contextCompression: ContextCompressionPresetConfigSchema.optional(),
  predefinedTools: PredefinedToolsPresetConfigSchema.optional(),
  predefinedPrompts: PredefinedPromptsPresetConfigSchema.optional(),
});

/**
 * Presets Configuration Type (inferred from schema)
 */
export type PresetsConfig = z.infer<typeof PresetsConfigSchema>;
/**
 * Context Compression Preset Configuration Type (inferred from schema)
 */
export type ContextCompressionPresetConfig = z.infer<typeof ContextCompressionPresetConfigSchema>;
/**
 * Predefined Tools Preset Configuration Type (inferred from schema)
 */
export type PredefinedToolsPresetConfig = z.infer<typeof PredefinedToolsPresetConfigSchema>;
/**
 * Predefined Prompts Preset Configuration Type (inferred from schema)
 */
export type PredefinedPromptsPresetConfig = z.infer<typeof PredefinedPromptsPresetConfigSchema>;

// ============================================================================
// Output Configuration Schemas
// ============================================================================

/**
 * Log Level Schema
 */
export const LogLevelSchema = z.enum(["error", "warn", "info", "debug"]);

/**
 * SDK Log Level Schema
 */
export const SDKLogLevelSchema = z.enum(["silent", "error", "warn", "info", "debug"]);

/**
 * Output Format Schema
 */
export const OutputFormatSchema = z.enum(["json", "table", "plain"]);

/**
 * Output Configuration Schema
 */
export const OutputConfigSchema = z.object({
  dir: z.string().min(1, "Output directory is required"),
  logFilePattern: z.string().min(1, "Log file pattern is required"),
  enableLogTerminal: z.boolean(),
  enableSDKLogs: z.boolean(),
  sdkLogLevel: SDKLogLevelSchema,
});

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Type guard for CompressionConfig
 */
export const isCompressionConfig = (
  config: unknown,
): config is z.infer<typeof CompressionConfigSchema> => {
  return CompressionConfigSchema.safeParse(config).success;
};

/**
 * Type guard for StorageConfig
 */
export const isStorageConfig = (config: unknown): config is z.infer<typeof StorageConfigSchema> => {
  return StorageConfigSchema.safeParse(config).success;
};

/**
 * Type guard for PresetsConfig
 */
export const isPresetsConfig = (
  config: unknown,
): config is z.infer<typeof PresetsConfigSchema> => {
  return PresetsConfigSchema.safeParse(config).success;
};

/**
 * Type guard for OutputConfig
 */
export const isOutputConfig = (
  config: unknown,
): config is z.infer<typeof OutputConfigSchema> => {
  return OutputConfigSchema.safeParse(config).success;
};
