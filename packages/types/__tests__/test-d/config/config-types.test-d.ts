/**
 * Configuration Types Test
 * 
 * Tests for configuration type definitions including Storage, Presets, Output, and Schemas.
 * These types define the configuration structure for SDK and applications.
 * 
 * Priority: 🟢 LOW (Stage 3)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  // Storage Config
  StorageConfig,
  StorageType,
  JsonStorageConfig,
  SqliteStorageConfig,
  CompressionConfig,
  CompressionAlgorithm,
  // Presets Config
  PresetsConfig,
  ContextCompressionPresetConfig,
  PredefinedToolsPresetConfig,
  PredefinedPromptsPresetConfig,
  // Output Config
  OutputConfig,
  LogLevel,
  SDKLogLevel,
  OutputFormat,
} from "../../../src/index.js";
import {
  isStorageConfig,
  isPresetsConfig,
  isOutputConfig,
  isCompressionConfig,
} from "../../../src/index.js";

// ============================================================================
// Test 1: Storage Configuration Types
// ============================================================================

/**
 * Test StorageType union
 */
declare const storageType: StorageType;
expectType<"json" | "sqlite" | "memory" | "postgres">(storageType);

/**
 * Test CompressionAlgorithm union
 */
declare const compressionAlgo: CompressionAlgorithm;
expectType<"gzip" | "brotli">(compressionAlgo);

/**
 * Test CompressionConfig interface
 */
const compression: CompressionConfig = {
  enabled: true,
  algorithm: "gzip",
  threshold: 1024,
};

expectType<CompressionConfig>(compression);
expectType<boolean>(compression.enabled);
expectType<CompressionAlgorithm>(compression.algorithm);
expectType<number>(compression.threshold);

/**
 * Test JsonStorageConfig interface
 */
const jsonConfig: JsonStorageConfig = {
  baseDir: "/path/to/storage",
  enableFileLock: true,
  compression: {
    enabled: true,
    algorithm: "brotli",
    threshold: 2048,
  },
};

expectType<JsonStorageConfig>(jsonConfig);
expectType<string>(jsonConfig.baseDir);
expectType<boolean>(jsonConfig.enableFileLock);
expectType<CompressionConfig | undefined>(jsonConfig.compression);

/**
 * Test JsonStorageConfig without optional compression
 */
const jsonConfigNoCompression: JsonStorageConfig = {
  baseDir: "/data",
  enableFileLock: false,
};

expectType<JsonStorageConfig>(jsonConfigNoCompression);

/**
 * Test SqliteStorageConfig interface
 */
const sqliteConfig: SqliteStorageConfig = {
  dbPath: "/path/to/database.db",
  enableWAL: true,
  enableLogging: true,
  readonly: false,
  fileMustExist: false,
  timeout: 5000,
};

expectType<SqliteStorageConfig>(sqliteConfig);
expectType<string>(sqliteConfig.dbPath);
expectType<boolean>(sqliteConfig.enableWAL);
expectType<boolean>(sqliteConfig.enableLogging);
expectType<boolean>(sqliteConfig.readonly);
expectType<boolean>(sqliteConfig.fileMustExist);
expectType<number>(sqliteConfig.timeout);

/**
 * Test StorageConfig discriminated union
 */
const jsonStorage: StorageConfig = {
  type: "json",
  json: {
    baseDir: "/data",
    enableFileLock: true,
  },
};

const sqliteStorage: StorageConfig = {
  type: "sqlite",
  sqlite: {
    dbPath: "/data.db",
    enableWAL: true,
    enableLogging: false,
    readonly: false,
    fileMustExist: false,
    timeout: 3000,
  },
};

const memoryStorage: StorageConfig = {
  type: "memory",
};

expectType<StorageConfig>(jsonStorage);
expectType<StorageConfig>(sqliteStorage);
expectType<StorageConfig>(memoryStorage);

/**
 * Test StorageConfig type narrowing
 */
if (jsonStorage.type === "json") {
  expectType<JsonStorageConfig | undefined>(jsonStorage.json);
}

if (sqliteStorage.type === "sqlite") {
  expectType<SqliteStorageConfig | undefined>(sqliteStorage.sqlite);
}

// ============================================================================
// Test 2: Presets Configuration Types
// ============================================================================

/**
 * Test ContextCompressionPresetConfig interface
 */
const contextCompression: ContextCompressionPresetConfig = {
  enabled: true,
  prompt: "Compress this context",
  timeout: 30000,
  maxTriggers: 10,
};

expectType<ContextCompressionPresetConfig>(contextCompression);
expectType<boolean>(contextCompression.enabled);
expectType<string | undefined>(contextCompression.prompt);
expectType<number | undefined>(contextCompression.timeout);
expectType<number | undefined>(contextCompression.maxTriggers);

/**
 * Test minimal ContextCompressionPresetConfig
 */
const minimalCompression: ContextCompressionPresetConfig = {
  enabled: false,
};

expectType<ContextCompressionPresetConfig>(minimalCompression);

/**
 * Test PredefinedToolsPresetConfig interface
 */
const predefinedTools: PredefinedToolsPresetConfig = {
  enabled: true,
  allowList: ["readFile", "writeFile"],
  blockList: ["bash"],
  config: {
    readFile: {
      workspaceDir: "/workspace",
      maxFileSize: 1048576, // 1MB
    },
    writeFile: {
      workspaceDir: "/workspace",
    },
    bash: {
      defaultTimeout: 30000,
      maxTimeout: 300000,
    },
  },
};

expectType<PredefinedToolsPresetConfig>(predefinedTools);
expectType<boolean>(predefinedTools.enabled);
expectType<string[] | undefined>(predefinedTools.allowList);
expectType<string[] | undefined>(predefinedTools.blockList);
expectType<
  | {
      readFile?: { workspaceDir?: string; maxFileSize?: number };
      writeFile?: { workspaceDir?: string };
      editFile?: { workspaceDir?: string };
      bash?: { defaultTimeout?: number; maxTimeout?: number };
      sessionNote?: { workspaceDir?: string; memoryFile?: string };
      backgroundShell?: { workspaceDir?: string };
    }
  | undefined
>(predefinedTools.config);

/**
 * Test minimal PredefinedToolsPresetConfig
 */
const minimalTools: PredefinedToolsPresetConfig = {
  enabled: false,
};

expectType<PredefinedToolsPresetConfig>(minimalTools);

/**
 * Test PredefinedPromptsPresetConfig interface
 */
const predefinedPrompts: PredefinedPromptsPresetConfig = {
  enabled: true,
};

expectType<PredefinedPromptsPresetConfig>(predefinedPrompts);
expectType<boolean>(predefinedPrompts.enabled);

/**
 * Test PresetsConfig interface with all fields
 */
const fullPresets: PresetsConfig = {
  contextCompression: {
    enabled: true,
    timeout: 30000,
  },
  predefinedTools: {
    enabled: true,
    allowList: ["readFile"],
  },
  predefinedPrompts: {
    enabled: false,
  },
};

expectType<PresetsConfig>(fullPresets);
expectType<ContextCompressionPresetConfig | undefined>(fullPresets.contextCompression);
expectType<PredefinedToolsPresetConfig | undefined>(fullPresets.predefinedTools);
expectType<PredefinedPromptsPresetConfig | undefined>(fullPresets.predefinedPrompts);

/**
 * Test empty PresetsConfig
 */
const emptyPresets: PresetsConfig = {};

expectType<PresetsConfig>(emptyPresets);

// ============================================================================
// Test 3: Output Configuration Types
// ============================================================================

/**
 * Test LogLevel union
 */
declare const logLevel: LogLevel;
expectType<"error" | "warn" | "info" | "debug">(logLevel);

/**
 * Test SDKLogLevel union
 */
declare const sdkLogLevel: SDKLogLevel;
expectType<"silent" | "error" | "warn" | "info" | "debug">(sdkLogLevel);

/**
 * Test OutputFormat union
 */
declare const outputFormat: OutputFormat;
expectType<"json" | "table" | "plain">(outputFormat);

/**
 * Test OutputConfig interface
 */
const outputConfig: OutputConfig = {
  dir: "/path/to/output",
  logFilePattern: "app-{date}.log",
  enableLogTerminal: true,
  enableSDKLogs: true,
  sdkLogLevel: "info",
};

expectType<OutputConfig>(outputConfig);
expectType<string>(outputConfig.dir);
expectType<string>(outputConfig.logFilePattern);
expectType<boolean>(outputConfig.enableLogTerminal);
expectType<boolean>(outputConfig.enableSDKLogs);
expectType<SDKLogLevel>(outputConfig.sdkLogLevel);

/**
 * Test OutputConfig with different log levels
 */
const silentOutput: OutputConfig = {
  dir: "/logs",
  logFilePattern: "log.log",
  enableLogTerminal: false,
  enableSDKLogs: false,
  sdkLogLevel: "silent",
};

const debugOutput: OutputConfig = {
  dir: "/logs",
  logFilePattern: "debug-{timestamp}.log",
  enableLogTerminal: true,
  enableSDKLogs: true,
  sdkLogLevel: "debug",
};

expectType<OutputConfig>(silentOutput);
expectType<OutputConfig>(debugOutput);

// ============================================================================
// Test 4: Type Guards
// ============================================================================

/**
 * Test isCompressionConfig type guard
 */
declare const unknownValue: unknown;

if (isCompressionConfig(unknownValue)) {
  expectType<CompressionConfig>(unknownValue);
  expectType<boolean>(unknownValue.enabled);
  expectType<CompressionAlgorithm>(unknownValue.algorithm);
  expectType<number>(unknownValue.threshold);
}

/**
 * Test isStorageConfig type guard
 */
if (isStorageConfig(unknownValue)) {
  expectAssignable<StorageConfig>(unknownValue);
  expectAssignable<StorageType>(unknownValue.type);
  
  if (unknownValue.type === "json") {
    expectType<JsonStorageConfig | undefined>(unknownValue.json);
  }
  
  if (unknownValue.type === "sqlite") {
    expectAssignable<SqliteStorageConfig | undefined>(unknownValue.sqlite);
  }
}

/**
 * Test isPresetsConfig type guard
 */
if (isPresetsConfig(unknownValue)) {
  expectAssignable<PresetsConfig>(unknownValue);
  expectAssignable<ContextCompressionPresetConfig | undefined>(unknownValue.contextCompression);
  expectAssignable<PredefinedToolsPresetConfig | undefined>(unknownValue.predefinedTools);
  expectAssignable<PredefinedPromptsPresetConfig | undefined>(unknownValue.predefinedPrompts);
}

/**
 * Test isOutputConfig type guard
 */
if (isOutputConfig(unknownValue)) {
  expectType<OutputConfig>(unknownValue);
  expectType<string>(unknownValue.dir);
  expectType<string>(unknownValue.logFilePattern);
  expectType<boolean>(unknownValue.enableLogTerminal);
  expectType<boolean>(unknownValue.enableSDKLogs);
  expectType<SDKLogLevel>(unknownValue.sdkLogLevel);
}

// ============================================================================
// Test 5: Combined Configuration - Real-world Scenarios
// ============================================================================

/**
 * Test complete application configuration
 */
interface AppConfig {
  storage: StorageConfig;
  presets: PresetsConfig;
  output: OutputConfig;
}

const appConfig: AppConfig = {
  storage: {
    type: "sqlite",
    sqlite: {
      dbPath: "./data/app.db",
      enableWAL: true,
      enableLogging: true,
      readonly: false,
      fileMustExist: false,
      timeout: 5000,
    },
  },
  presets: {
    contextCompression: {
      enabled: true,
      timeout: 30000,
      maxTriggers: 5,
    },
    predefinedTools: {
      enabled: true,
      allowList: ["readFile", "writeFile", "editFile"],
      config: {
        readFile: {
          workspaceDir: "./workspace",
          maxFileSize: 5242880, // 5MB
        },
      },
    },
    predefinedPrompts: {
      enabled: true,
    },
  },
  output: {
    dir: "./logs",
    logFilePattern: "app-{date}.log",
    enableLogTerminal: true,
    enableSDKLogs: true,
    sdkLogLevel: "info",
  },
};

expectType<AppConfig>(appConfig);
expectType<StorageConfig>(appConfig.storage);
expectType<PresetsConfig>(appConfig.presets);
expectType<OutputConfig>(appConfig.output);

/**
 * Test configuration with JSON storage
 */
const jsonAppConfig: AppConfig = {
  storage: {
    type: "json",
    json: {
      baseDir: "./data/json",
      enableFileLock: true,
      compression: {
        enabled: true,
        algorithm: "gzip",
        threshold: 4096,
      },
    },
  },
  presets: {},
  output: {
    dir: "./logs",
    logFilePattern: "app.log",
    enableLogTerminal: false,
    enableSDKLogs: false,
    sdkLogLevel: "error",
  },
};

expectType<AppConfig>(jsonAppConfig);

/**
 * Test configuration with memory storage (simplest)
 */
const memoryAppConfig: AppConfig = {
  storage: {
    type: "memory",
  },
  presets: {
    predefinedTools: {
      enabled: false,
    },
  },
  output: {
    dir: "./output",
    logFilePattern: "{timestamp}.log",
    enableLogTerminal: true,
    enableSDKLogs: true,
    sdkLogLevel: "debug",
  },
};

expectType<AppConfig>(memoryAppConfig);

// ============================================================================
// Test 6: Optional Fields and Partial Configurations
// ============================================================================

/**
 * Test partial presets configuration
 */
const partialPresets: PresetsConfig = {
  contextCompression: {
    enabled: true,
  },
  // predefinedTools and predefinedPrompts are omitted
};

expectType<PresetsConfig>(partialPresets);

/**
 * Test storage config with optional fields omitted
 */
const minimalJsonStorage: StorageConfig = {
  type: "json",
  json: {
    baseDir: "/tmp",
    enableFileLock: false,
    // compression is optional
  },
};

expectType<StorageConfig>(minimalJsonStorage);

/**
 * Test tools preset with nested optional configs
 */
const toolsWithPartialConfig: PredefinedToolsPresetConfig = {
  enabled: true,
  config: {
    readFile: {
      // workspaceDir is optional
      maxFileSize: 1024,
    },
    // Other tool configs are omitted
  },
};

expectType<PredefinedToolsPresetConfig>(toolsWithPartialConfig);

// ============================================================================
// Test 7: Type Assignability
// ============================================================================

/**
 * Test that specific storage types are assignable to StorageConfig
 */
expectAssignable<StorageConfig>({
  type: "json",
  json: {
    baseDir: "/data",
    enableFileLock: true,
  },
});

expectAssignable<StorageConfig>({
  type: "sqlite",
  sqlite: {
    dbPath: "/data.db",
    enableWAL: true,
    enableLogging: true,
    readonly: false,
    fileMustExist: false,
    timeout: 3000,
  },
});

expectAssignable<StorageConfig>({
  type: "memory",
});

/**
 * Test that valid log levels are assignable
 */
expectAssignable<SDKLogLevel>("silent");
expectAssignable<SDKLogLevel>("error");
expectAssignable<SDKLogLevel>("warn");
expectAssignable<SDKLogLevel>("info");
expectAssignable<SDKLogLevel>("debug");

/**
 * Test that valid compression algorithms are assignable
 */
expectAssignable<CompressionAlgorithm>("gzip");
expectAssignable<CompressionAlgorithm>("brotli");
