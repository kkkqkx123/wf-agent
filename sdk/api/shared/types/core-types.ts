/**
 * Core API Type Definitions
 * Defines types related to core execution
 */

import type { WorkflowExecutionOptions } from "@wf-agent/types";
import type {
  CheckpointStorageCallback,
  WorkflowStorageCallback,
  WorkflowExecutionStorageCallback,
  TaskStorageCallback,
} from "@wf-agent/storage";

/**
 * SDK Options
 */
export interface SDKOptions {
  /** Whether to enable debug mode */
  debug?: boolean;
  /** Log Level */
  logLevel?: "debug" | "info" | "warn" | "error";
  /** Default timeout period (in milliseconds) */
  defaultTimeout?: number;
  /** Whether to enable checkpoints */
  enableCheckpoints?: boolean;
  /** Checkpoint storage callback interface (implemented by the application layer) */
  checkpointStorageCallback?: CheckpointStorageCallback;
  /** Workflow storage callback interface (implemented by the application layer) */
  workflowStorageCallback?: WorkflowStorageCallback;
  /** Task storage callback interface (implemented by the application layer) */
  taskStorageCallback?: TaskStorageCallback;
  /** Whether to enable verification */
  enableValidation?: boolean;
  /** Predefined feature options */
  presets?: {
    /** Context Compression Function Configuration */
    contextCompression?: {
      enabled?: boolean;
      prompt?: string;
      timeout?: number;
      maxTriggers?: number;
    };
    /** Predefined tool configuration */
    predefinedTools?: {
      /** Whether to enable predefined tools (default is true) */
      enabled?: boolean;
      /** Only enable the specified tools (allowlist). */
      allowList?: string[];
      /** Disable the specified tool (blocklist it). */
      blockList?: string[];
      /** Tool-specific configuration */
      config?: {
        readFile?: { workspaceDir?: string; maxFileSize?: number };
        writeFile?: { workspaceDir?: string };
        editFile?: { workspaceDir?: string };
        bash?: { defaultTimeout?: number; maxTimeout?: number };
        sessionNote?: { workspaceDir?: string; memoryFile?: string };
        backgroundShell?: { workspaceDir?: string };
      };
    };
    /** Predefined prompt word template configuration */
    predefinedPrompts?: {
      /** Whether to enable the predefined prompt word template (default is true) */
      enabled?: boolean;
    };
  };
}

/**
 * SDK Dependencies
 */
export interface SDKDependencies {
  /** Workflow Registry */
  workflowRegistry?: unknown;
  /** Workflow Execution Registry */
  executionRegistry?: unknown;
  /** Tool Registry */
  toolRegistry?: unknown;
  /** Script Registry */
  scriptRegistry?: unknown;
  /** Event Manager */
  eventManager?: unknown;
  /** Checkpoint storage callback interface (implemented by the application layer) */
  checkpointStorageCallback?: CheckpointStorageCallback;
}

export type { WorkflowExecutionOptions };
