/**
 * Script Module Type Definition
 * Defines basic information and configuration for script execution
 */

import type { ID, Metadata } from "../common.js";

/**
 * Script type
 */
export type ScriptType =
  /** Shell Scripts */
  | "SHELL"
  /** CMD Screenplay */
  | "CMD"
  /** PowerShell scripts */
  | "POWERSHELL"
  /** Python script */
  | "PYTHON"
  /** JavaScript Scripts */
  | "JAVASCRIPT";

/**
 * Script execution options
 */
export interface ScriptExecutionOptions {
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Retries */
  retries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Whether to enable exponential backoff */
  exponentialBackoff?: boolean;
  /** Job Catalog */
  workingDirectory?: string;
  /** environment variable */
  environment?: Record<string, string>;
  /** Whether to enable sandboxing */
  sandbox?: boolean;
  /** Sandbox Configuration */
  sandboxConfig?: SandboxConfig;
  /** Abort signal (for canceling execution) */
  signal?: AbortSignal;
}

/**
 * Sandbox Configuration
 */
export interface SandboxConfig {
  /** Sandbox Type */
  type: "docker" | "nodejs" | "python" | "custom";
  /** Sandbox mirrors or environments */
  image?: string;
  /** Resource constraints */
  resourceLimits?: {
    /** Memory Limit (MB) */
    memory?: number;
    /** CPU limit (number of cores) */
    cpu?: number;
    /** Disk limit (MB) */
    disk?: number;
  };
  /** Network Configuration */
  network?: {
    /** Network enabled or not */
    enabled: boolean;
    /** List of Allowed Domains */
    allowedDomains?: string[];
  };
  /** File System Access Configuration */
  filesystem?: {
    /** List of allowed access paths */
    allowedPaths?: string[];
    /** Read-only or not */
    readOnly?: boolean;
  };
}

/**
 * Script execution results
 */
export interface ScriptExecutionResult {
  /** Successful implementation */
  success: boolean;
  /** screenplay title */
  scriptName: string;
  /** Script type */
  scriptType: ScriptType;
  /** standard output */
  stdout?: string;
  /** standard error */
  stderr?: string;
  /** exit code */
  exitCode?: number;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** error message */
  error?: string;
  /** Implementation of environmental information */
  environment?: Record<string, unknown>;
  /** Retries */
  retryCount?: number;
}

/**
 * Script Definition
 */
export interface Script {
  /** Script Unique Identifier */
  id: ID;
  /** screenplay title */
  name: string;
  /** Script type */
  type: ScriptType;
  /** Script Description */
  description: string;
  /** Script content (inline code) */
  content?: string;
  /** Script file path (external file) */
  filePath?: string;
  /** Script execution options */
  options: ScriptExecutionOptions;
  /** Script Metadata */
  metadata?: ScriptMetadata;
  /** Enable or not (default is true) */
  enabled?: boolean;
}

/**
 * Script Metadata
 */
export interface ScriptMetadata {
  /** Script Category */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** author */
  author?: string;
  /** releases */
  version?: string;
  /** Document URL */
  documentationUrl?: string;
  /** Custom Fields */
  customFields?: Metadata;
}
