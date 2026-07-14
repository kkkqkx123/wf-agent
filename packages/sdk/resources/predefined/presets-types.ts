/**
 * Presets Type Definitions
 *
 * Defines types for presets configuration — context compression,
 * predefined tools, and predefined prompts.
 *
 * Tool config sub-types reference the full definitions from
 * tools/types.ts to avoid duplication and drift.
 */

import type {
  ReadFileConfig,
  WriteFileConfig,
  EditFileConfig,
  RunShellConfig,
  SessionNoteConfig,
  BackendShellConfig,
} from "./tools/types.js";
import type { CustomResourcesPresetConfig } from "../custom/types.js";

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
 *
 * References the full tool config types from tools/types.ts
 * to eliminate duplication with the individual tool config
 * interfaces (ReadFileConfig, WriteFileConfig, etc.).
 */
export interface PredefinedToolsPresetConfig {
  enabled: boolean;
  allowList?: string[];
  blockList?: string[];
  config?: {
    readFile?: ReadFileConfig;
    writeFile?: WriteFileConfig;
    editFile?: EditFileConfig;
    runShell?: RunShellConfig;
    sessionNote?: SessionNoteConfig;
    backendShell?: BackendShellConfig;
  };
}

/**
 * Predefined Prompts Preset Configuration
 */
export interface PredefinedPromptsPresetConfig {
  enabled: boolean;
}

/**
 * Predefined Tool Descriptions Preset Configuration
 */
export interface PredefinedToolDescriptionsPresetConfig {
  enabled: boolean;
}

/**
 * Presets Configuration (resolved form, after defaults applied)
 */
export interface PresetsConfig {
  contextCompression?: ContextCompressionPresetConfig;
  predefinedTools?: PredefinedToolsPresetConfig;
  predefinedPrompts?: PredefinedPromptsPresetConfig;
  predefinedToolDescriptions?: PredefinedToolDescriptionsPresetConfig;
  customResources?: CustomResourcesPresetConfig;
}

/**
 * Input type for PresetsConfig (all fields optional, for config file loading)
 */
export interface PresetsConfigInput {
  contextCompression?: {
    enabled?: boolean;
    prompt?: string;
    timeout?: number;
    maxTriggers?: number;
  };
  predefinedTools?: {
    enabled?: boolean;
    allowList?: string[];
    blockList?: string[];
    config?: {
      readFile?: {
        workspaceDir?: string;
        maxFileSize?: number;
        maxChars?: number;
        maxLines?: number;
        enableIgnore?: boolean;
        enableProtect?: boolean;
        modelId?: string;
      };
      writeFile?: {
        workspaceDir?: string;
        enableProtect?: boolean;
      };
      editFile?: {
        workspaceDir?: string;
        enableProtect?: boolean;
      };
      runShell?: {
        defaultTimeout?: number;
        maxTimeout?: number;
      };
      sessionNote?: {
        workspaceDir?: string;
        dbPath?: string;
        sessionId?: string;
        maxNotes?: number;
      };
      backendShell?: {
        workspaceDir?: string;
        maxBackgroundTimeout?: number;
      };
    };
  };
  predefinedPrompts?: {
    enabled?: boolean;
  };
  predefinedToolDescriptions?: {
    enabled?: boolean;
  };
  customResources?: {
    enabled?: boolean;
    toolsPath?: string;
    triggersPath?: string;
    promptsPath?: string;
    validationLevel?: "strict" | "lenient";
  };
}
