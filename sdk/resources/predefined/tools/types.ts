/**
 * Defined Tool Type Definitions
 *
 * Expand the types related to tool definitions for the configuration and registration of predefined tools.
 */

import type { ToolParameterSchema, ToolType } from "@wf-agent/types";
import type { BuiltinToolsOptions } from "./builtin/types.js";
import type { ShellPolicy, SandboxConfig } from "@wf-agent/types";
import type { SkillHandlerConfig } from "./builtin/interaction/skill/index.js";

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
    skill?: SkillHandlerConfig;
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
  /** Maximum file size (in bytes) */
  maxFileSize?: number;
  /** Default character limit per read (default: 50000). Acts as a hard cap to prevent excessive output. */
  maxChars?: number;
  /** Default maximum lines per read (default: 2000) */
  maxLines?: number;
  /** Enable ignore filtering */
  enableIgnore?: boolean;
  /** Enable write protection */
  enableProtect?: boolean;
  /** Model ID for conditional processing (e.g., HTML entity unescaping) */
  modelId?: string;
  /**
   * VFS file I/O provider.
   * When set, read operations check VFS first, falling back to Host FS.
   */
  vfs?: VFSFileIO;
}

/**
 * Minimal VFS file I/O interface for file editing tools.
 *
 * When a VFS instance is provided, file editing tools route operations
 * through VFS instead of directly accessing the host filesystem.
 * This ensures consistency between tool-driven edits and sandbox script
 * file operations, especially when VFS sync-to-host mode is enabled.
 */
export interface VFSFileIO {
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  stat(path: string): Promise<{ name: string; type: "file" | "directory"; size: number } | null>;
  exists(path: string): Promise<boolean>;
  mkdir(path: string): Promise<void>;
  remove(path: string): Promise<void>;
}

/**
 * File writing tool configuration
 */
export interface WriteFileConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Enable write protection */
  enableProtect?: boolean;
  /**
   * VFS file I/O provider.
   * When set, all write operations route through VFS instead of direct Host FS.
   * This is the key integration point for Plan A: "VFS as unified middle layer".
   */
  vfs?: VFSFileIO;
}

/**
 * File Editing Tool Configuration
 */
export interface EditFileConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Enable write protection */
  enableProtect?: boolean;
  /**
   * VFS file I/O provider.
   * When set, all read/write operations route through VFS.
   */
  vfs?: VFSFileIO;
}

/**
 * Run Shell Tool Configuration
 */
export interface RunShellConfig {
  /** Default timeout value (in milliseconds) */
  defaultTimeout?: number;
  /** Maximum timeout period (in milliseconds) */
  maxTimeout?: number;
  /** Shell policy for static analysis pre-check */
  shellPolicy?: ShellPolicy;
  /** Sandbox configuration for unified sandbox runtime integration */
  sandboxConfig?: SandboxConfig;
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
  /** Maximum runtime for background commands (ms), 0 = no limit, default: 3600000 (1h) */
  maxBackgroundTimeout?: number;
  /** Shell policy for static analysis pre-check */
  shellPolicy?: ShellPolicy;
}
