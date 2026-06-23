/**
 * Tool Runtime Configuration Type Definition
 * (Execution-time configuration for different tool types)
 */

export type ToolExecutionResult = unknown;

/**
 * Runtime tool parameters (actual parameter values passed during execution)
 * This is a simple key-value record.
 */
export type ToolParameterSchema = Record<string, unknown>;

/**
 * Stateless Tool Configuration
 */
export interface StatelessToolConfig {
  /** executable function */
  execute: (parameters: ToolParameterSchema) => Promise<ToolExecutionResult>;
  /** releases */
  version?: string;
  /** descriptive */
  description?: string;
}

/**
 * Stateful tool instance interface
 */
export interface StatefulToolInstance {
  /** Tool execution method */
  execute: (parameters: ToolParameterSchema) => Promise<ToolExecutionResult>;
  /** Optional cleanup method */
  destroy?: () => void;
}

/**
 * Stateful Tool Factory
 */
export interface StatefulToolFactory {
  /** Creating a Tool Example */
  create(): StatefulToolInstance;
}

/**
 * Stateful Tool Configuration
 */
export interface StatefulToolConfig {
  /** factory function */
  factory: StatefulToolFactory;
}

/**
 * REST Tool Configuration
 */
export interface RestToolConfig {
  /** Base URL */
  baseUrl?: string;
  /** request header */
  headers?: Record<string, string>;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
}

/**
 * Built-in Tool Configuration
 */
export interface BuiltinToolConfig {
  /** Execute function with context */
  execute: (
    parameters: ToolParameterSchema,
    context: BuiltinToolExecutionContext,
  ) => Promise<ToolExecutionResult>;
}

/**
 * Built-in Tool Execution Context
 * Provides context information for built-in tool execution
 *
 * Design Principles:
 * - Provides minimal context interface for builtin tools
 * - SDK-specific types should be defined in SDK internal type files
 * - This interface uses 'unknown' to avoid circular dependencies
 */
export interface BuiltinToolExecutionContext {
  /** Current execution ID */
  executionId?: string;
  /** Parent execution entity */
  parentExecutionEntity?: unknown;
  /** Execution registry */
  executionRegistry?: unknown;
  /** Event manager */
  eventManager?: unknown;
  /** Execution builder */
  executionBuilder?: unknown;
  /** Task queue manager */
  taskQueueManager?: unknown;
  /** Workflow registry */
  workflowRegistry?: unknown;
  /** Graph registry */
  graphRegistry?: unknown;
}

/**
 * MCP Tool Configuration
 */
export interface McpToolConfig {
  /** MCP Server name */
  serverName: string;
  /** Tool name on the MCP server */
  toolName: string;
  /** Optional: timeout in milliseconds (overrides server timeout) */
  timeout?: number;
  /** Optional: whether to create checkpoints on success */
  createCheckpointOnSuccess?: boolean;
  /** Optional: checkpoint description template */
  checkpointDescriptionTemplate?: string;
}

/**
 * Tool Runtime Configuration Type (Union of all tool types)
 */
export type ToolConfig =
  | StatelessToolConfig
  | StatefulToolConfig
  | RestToolConfig
  | BuiltinToolConfig
  | McpToolConfig;
