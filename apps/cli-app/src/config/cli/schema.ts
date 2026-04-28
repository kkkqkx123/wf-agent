/**
 * CLI Configuration Schema
 * Zod schema definitions for configuration validation.
 */

import { z } from "zod";
import type {
  CLIConfig,
  CompressionConfig,
  JsonStorageConfig,
  SqliteStorageConfig,
  StorageConfig,
  OutputConfig,
  ContextCompressionPresetConfig,
  PredefinedToolsPresetConfig,
  PredefinedPromptsPresetConfig,
  PresetsConfig,
} from "./types.js";

/**
 * Compression Configuration Schema
 */
export const CompressionConfigSchema = z.object({
  enabled: z.boolean().default(false),
  algorithm: z.enum(["gzip", "brotli", "zlib"]).default("gzip"),
  threshold: z.number().default(1024),
}) satisfies z.ZodType<CompressionConfig>;

/**
 * JSON Storage Configuration Schema
 */
export const JsonStorageConfigSchema = z.object({
  baseDir: z.string().default("./storage"),
  enableFileLock: z.boolean().default(false),
  compression: CompressionConfigSchema.optional(),
}) satisfies z.ZodType<JsonStorageConfig>;

/**
 * SQLite Storage Configuration Schema
 */
export const SqliteStorageConfigSchema = z.object({
  dbPath: z.string().default("./storage/cli-app.db"),
  enableWAL: z.boolean().default(true),
  enableLogging: z.boolean().default(false),
  readonly: z.boolean().default(false),
  fileMustExist: z.boolean().default(false),
  timeout: z.number().positive().default(5000),
}) satisfies z.ZodType<SqliteStorageConfig>;

/**
 * Storage Configuration Schema
 */
export const StorageConfigSchema = z.object({
  type: z.enum(["json", "sqlite", "memory"]).default("json"),
  json: JsonStorageConfigSchema.optional(),
  sqlite: SqliteStorageConfigSchema.optional(),
}) satisfies z.ZodType<StorageConfig>;

/**
 * Output Configuration Schema
 */
export const OutputConfigSchema = z.object({
  dir: z.string().default("./outputs"),
  logFilePattern: z.string().default("cli-app-{date}.log"),
  enableLogTerminal: z.boolean().default(true),
  enableSDKLogs: z.boolean().default(true),
  sdkLogLevel: z.enum(["silent", "error", "warn", "info", "debug"]).default("silent"),
}) satisfies z.ZodType<OutputConfig>;

/**
 * Context Compression Preset Configuration Schema
 */
export const ContextCompressionPresetConfigSchema = z.object({
  enabled: z.boolean().default(true),
  prompt: z.string().optional(),
  timeout: z.number().optional(),
  maxTriggers: z.number().optional(),
}) satisfies z.ZodType<ContextCompressionPresetConfig>;

/**
 * Predefined Tools Preset Configuration Schema
 */
export const PredefinedToolsPresetConfigSchema = z.object({
  enabled: z.boolean().default(true),
  allowList: z.array(z.string()).optional(),
  blockList: z.array(z.string()).optional(),
  config: z
    .object({
      readFile: z
        .object({
          workspaceDir: z.string().optional(),
          maxFileSize: z.number().optional(),
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
          defaultTimeout: z.number().optional(),
          maxTimeout: z.number().optional(),
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
}) satisfies z.ZodType<PredefinedToolsPresetConfig>;

/**
 * Predefined Prompts Preset Configuration Schema
 */
export const PredefinedPromptsPresetConfigSchema = z.object({
  enabled: z.boolean().default(true),
}) satisfies z.ZodType<PredefinedPromptsPresetConfig>;

/**
 * Presets Configuration Schema
 */
export const PresetsConfigSchema = z.object({
  contextCompression: ContextCompressionPresetConfigSchema.optional(),
  predefinedTools: PredefinedToolsPresetConfigSchema.optional(),
  predefinedPrompts: PredefinedPromptsPresetConfigSchema.optional(),
}) satisfies z.ZodType<PresetsConfig>;

/**
 * Complete Configuration Schema
 */
export const CLIConfigSchema = z.object({
  apiUrl: z.string().url().optional(),
  apiKey: z.string().optional(),
  defaultTimeout: z.number().positive().default(30000),
  verbose: z.boolean().default(false),
  debug: z.boolean().default(false),
  logLevel: z.enum(["error", "warn", "info", "debug"]).default("warn"),
  outputFormat: z.enum(["json", "table", "plain"]).default("table"),
  maxConcurrentThreads: z.number().positive().default(5),
  storage: StorageConfigSchema.optional(),
  output: OutputConfigSchema.optional(),
  presets: PresetsConfigSchema.optional(),
}) satisfies z.ZodType<CLIConfig>;

/**
 * Type inference from schema (for runtime validation)
 */
export type CLIConfigValidated = z.infer<typeof CLIConfigSchema>;
