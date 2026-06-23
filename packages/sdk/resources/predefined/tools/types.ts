/**
 * Defined Tool Type Definitions
 *
 * Expand the types related to tool definitions for the configuration and registration of predefined tools.
 */

import type { ToolParameterSchema, ToolType } from "@wf-agent/types";
import type { BuiltinToolsOptions } from "./builtin/types.js";
import type { ShellPolicy, SandboxConfig } from "@wf-agent/types";
import type { SkillHandlerConfig } from "./builtin/skill/index.js";

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
    listFiles?: ListFilesConfig;
    glob?: GlobConfig;
    runShell?: RunShellConfig;
    sessionNote?: SessionNoteConfig;
    backendShell?: BackendShellConfig;
    skill?: SkillHandlerConfig;
  };
  /** Builtin tools options */
  builtin?: BuiltinToolsOptions;
  /** MCP Manager getter for dynamic use_mcp description generation */
  getMcpManager?: () => Promise<Record<string, unknown>>;
}

/**
 * File reading tool configuration
 *
 * Read operations always go directly to the host filesystem via HostFSAdapter.
 * The vfs field here is retained for symmetry in the config interface; when
 * a value is provided, it is used for the write guard (WriteGuardVFS) in cases
 * where the read tool handler also supports write operations (e.g., apply-patch,
 * apply-diff). Pure reads ignore the vfs field entirely.
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
   * VFS file I/O provider for write guard (apply-patch, apply-diff).
   * Pure reads (read_file, list_files, grep) always use HostFSAdapter directly.
   */
  vfs?: VFSFileIO;
}

/**
 * File listing tool configuration (list_files)
 */
export interface ListFilesConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Maximum results returned per call (default: 1000). Injected into tool description at registration time. */
  maxResults?: number;
  /** Enable ignore filtering to skip node_modules, .git, etc. LLM can override at call time via includeIgnored param. */
  enableIgnore?: boolean;
}

/**
 * Glob tool configuration (glob)
 */
export interface GlobConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Maximum results returned per call (default: 50). Injected into tool description at registration time. */
  maxResults?: number;
  /** Enable ignore filtering to skip node_modules, .git, etc. LLM can override at call time via includeIgnored param. */
  enableIgnore?: boolean;
}

/**
 * Minimal VFS file I/O interface for file editing tools.
 *
 * All tools use this interface instead of calling fs/promises directly,
 * providing a single integration point for VFS without tool-level branching.
 *
 * Read-write separation:
 *   Read operations (readFile, stat, exists, readdir) go to the host
 *   filesystem directly via HostFSAdapter — there is no middle layer.
 *
 *   Write operations (writeFile, mkdir, remove, rename) are routed through
 *   the sandbox VFS (e.g., SandboxVFS) for path policy enforcement when
 *   sandbox mode is enabled. When no sandbox is configured, writes also
 *   go to the host filesystem directly.
 *
 * When a WriteGuardVFS is provided via config.vfs, the split is automatic:
 * reads bypass VFS, writes pass through policy checks.
 *
 * Usage:
 *   const vfs = config.vfs ?? new HostFSAdapter(); // in tool handlers
 *   await vfs.readFile(path);  // direct FS
 *   await vfs.writeFile(path); // policy-guarded when sandbox enabled
 */
export interface VFSFileIO {
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  stat(path: string): Promise<{ name: string; type: "file" | "directory"; size: number } | null>;
  exists(path: string): Promise<boolean>;
  mkdir(path: string): Promise<void>;
  remove(path: string): Promise<void>;
  /** Rename/move a file from oldPath to newPath */
  rename(oldPath: string, newPath: string): Promise<void>;
  /** Read directory contents, returns entry names */
  readdir(path: string): Promise<string[]>;
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
   * VFS file I/O provider for write operations.
   * When set, writeFile is routed through VFS (WriteGuardVFS) for path
   * policy enforcement. Read operations always use HostFSAdapter directly.
   *
   * @see WriteGuardVFS
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
   * VFS file I/O provider for write operations.
   * When set, writeFile and rename are routed through VFS (WriteGuardVFS)
   * for path policy enforcement. Read operations always use HostFSAdapter.
   *
   * @see WriteGuardVFS
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
