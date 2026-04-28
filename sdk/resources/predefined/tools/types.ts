/**
 * Defined Tool Type Definitions
 *
 * Expand the types related to tool definitions for the configuration and registration of predefined tools.
 */

import type { ToolParameterSchema, ToolType } from "@wf-agent/types";
import type { BuiltinToolsOptions } from "./builtin/types.js";

/**
 * Tool Classification
 */
export type ToolCategory =
  | "filesystem" // File operations
  | "shell" // Shell execution
  | "memory" // Memory/Session Management
  | "code" // Code-related
  | "http"; // HTTP Request

/**
 * Predefined tool definitions
 */
export interface PredefinedToolDefinition {
  /** Tool ID */
  id: string;
  /** Tool Name */
  name: string;
  /** Tool Type */
  type: ToolType;
  /** Tool Classification */
  category: ToolCategory;
  /** Tool Description */
  description: string;
  /** Parameter Schema */
  parameters: ToolParameterSchema;
  /** Create a factory for executing functions */
  createHandler: (config?: Record<string, unknown>) => (...args: unknown[]) => Promise<unknown>;
  /** Create a factory for stateful instances (only of the STATEFUL type) */
  factory?: (config?: Record<string, unknown>) => {
    execute: (params: Record<string, unknown>) => Promise<unknown>;
    cleanup?: () => void;
  };
}

/**
 * Predefined tool configuration options
 */
export interface PredefinedToolsOptions {
  /** Only enable the specified tools (allowlist). */
  allowList?: string[];
  /** Disable the specified tool (blocklist it). */
  blockList?: string[];
  /** Tool-specific configuration */
  config?: {
    readFile?: ReadFileConfig;
    writeFile?: WriteFileConfig;
    editFile?: EditFileConfig;
    runShell?: RunShellConfig;
    sessionNote?: SessionNoteConfig;
    backendShell?: BackendShellConfig;
  };
  /** Builtin tools options */
  builtin?: BuiltinToolsOptions;
}

/**
 * File reading tool configuration
 */
export interface ReadFileConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Maximum file size (in number of characters) */
  maxFileSize?: number;
  /** Enable ignore filtering */
  enableIgnore?: boolean;
  /** Enable write protection */
  enableProtect?: boolean;
}

/**
 * File writing tool configuration
 */
export interface WriteFileConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Enable write protection */
  enableProtect?: boolean;
}

/**
 * File Editing Tool Configuration
 */
export interface EditFileConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Enable write protection */
  enableProtect?: boolean;
}

/**
 * Run Shell Tool Configuration
 */
export interface RunShellConfig {
  /** Default timeout value (in milliseconds) */
  defaultTimeout?: number;
  /** Maximum timeout period (in milliseconds) */
  maxTimeout?: number;
}

/**
 * Session Notes Tool Configuration
 */
export interface SessionNoteConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Memory file path */
  memoryFile?: string;
}

/**
 * Backend Shell Tool Configuration
 */
export interface BackendShellConfig {
  /** Working directory */
  workspaceDir?: string;
}
