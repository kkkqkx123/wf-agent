/**
 * Preset Configuration Types
 * Unified preset configuration types for enabling/disabling predefined resources
 */

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
