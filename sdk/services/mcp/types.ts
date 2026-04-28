/**
 * MCP Service Type Definitions
 * Core types for MCP server connection and management
 */

/**
 * MCP Server Status
 */
export type McpServerStatus = "connecting" | "connected" | "disconnected";

/**
 * MCP Transport Type
 */
export type McpTransportType = "stdio" | "sse" | "streamable-http";

/**
 * MCP Server Source
 */
export type McpServerSource = "global" | "project";

/**
 * Base MCP Server Configuration
 */
export interface McpServerConfigBase {
  /** Server type */
  type?: McpTransportType;
  /** Whether the server is disabled */
  disabled?: boolean;
  /** Timeout in seconds (1-3600, default 60) */
  timeout?: number;
  /** Tools that are always allowed without approval */
  alwaysAllow?: string[];
  /** Tools that are disabled */
  disabledTools?: string[];
  /** Paths to watch for changes and restart server */
  watchPaths?: string[];
}

/**
 * Stdio MCP Server Configuration
 */
export interface McpStdioConfig extends McpServerConfigBase {
  type: "stdio";
  /** Command to execute */
  command: string;
  /** Command arguments */
  args?: string[];
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
}

/**
 * SSE MCP Server Configuration
 */
export interface McpSseConfig extends McpServerConfigBase {
  type: "sse";
  /** Server URL */
  url: string;
  /** HTTP headers */
  headers?: Record<string, string>;
}

/**
 * Streamable HTTP MCP Server Configuration
 */
export interface McpStreamableHttpConfig extends McpServerConfigBase {
  type: "streamable-http";
  /** Server URL */
  url: string;
  /** HTTP headers */
  headers?: Record<string, string>;
}

/**
 * Unified MCP Server Configuration
 */
export type McpServerConfig = McpStdioConfig | McpSseConfig | McpStreamableHttpConfig;

/**
 * MCP Tool Definition
 */
export interface McpTool {
  /** Tool name */
  name: string;
  /** Tool description */
  description?: string;
  /** Input schema (JSON Schema) */
  inputSchema?: Record<string, unknown>;
  /** Whether this tool is always allowed */
  alwaysAllow?: boolean;
  /** Whether this tool is enabled for prompt */
  enabledForPrompt?: boolean;
}

/**
 * MCP Resource Definition
 */
export interface McpResource {
  /** Resource URI */
  uri: string;
  /** Resource name */
  name: string;
  /** Resource description */
  description?: string;
  /** MIME type */
  mimeType?: string;
}

/**
 * MCP Resource Template Definition
 */
export interface McpResourceTemplate {
  /** Template URI pattern */
  uriTemplate: string;
  /** Template name */
  name: string;
  /** Template description */
  description?: string;
  /** MIME type */
  mimeType?: string;
}

/**
 * MCP Error History Entry
 */
export interface McpErrorEntry {
  /** Error message */
  message: string;
  /** Timestamp */
  timestamp: number;
  /** Log level */
  level: "error" | "warn" | "info";
}

/**
 * MCP Server State
 */
export interface McpServerState {
  /** Server name */
  name: string;
  /** Server configuration (JSON string) */
  config: string;
  /** Connection status */
  status: McpServerStatus;
  /** Whether the server is disabled */
  disabled?: boolean;
  /** Server source */
  source: McpServerSource;
  /** Project path (for project-level servers) */
  projectPath?: string;
  /** Available tools */
  tools?: McpTool[];
  /** Available resources */
  resources?: McpResource[];
  /** Available resource templates */
  resourceTemplates?: McpResourceTemplate[];
  /** Server instructions */
  instructions?: string;
  /** Current error message */
  error?: string;
  /** Error history */
  errorHistory: McpErrorEntry[];
}

/**
 * MCP Tool Call Result
 */
export interface McpToolCallResult {
  /** Result content */
  content: Array<{
    type: "text" | "image" | "resource";
    text?: string;
    data?: string;
    mimeType?: string;
    resource?: {
      uri: string;
      mimeType?: string;
      text?: string;
      blob?: string;
    };
  }>;
  /** Whether the result is an error */
  isError?: boolean;
}

/**
 * MCP Resource Read Result
 */
export interface McpResourceReadResult {
  /** Resource contents */
  contents: Array<{
    uri: string;
    mimeType?: string;
    text?: string;
    blob?: string;
  }>;
}

/**
 * MCP Settings File Structure
 */
export interface McpSettings {
  /** MCP servers configuration */
  mcpServers: Record<string, McpServerConfig>;
}

/**
 * MCP Connection State (discriminated union)
 */
export type McpConnectionState =
  | { type: "connected"; server: McpServerState }
  | { type: "disconnected"; server: McpServerState };

/**
 * MCP Manager Options
 */
export interface McpManagerOptions {
  /** Whether MCP is enabled globally */
  mcpEnabled?: boolean;
  /** Maximum error history size */
  maxErrorHistory?: number;
  /** Connection timeout in milliseconds */
  connectionTimeout?: number;
  /** Debounce delay for config changes in milliseconds */
  configDebounceDelay?: number;
}

/**
 * MCP Event Types
 */
export type McpEventType =
  | "server:connecting"
  | "server:connected"
  | "server:disconnected"
  | "server:error"
  | "servers:changed";

/**
 * MCP Event Handler
 */
export type McpEventHandler = (event: {
  type: McpEventType;
  serverName?: string;
  data?: unknown;
}) => void;
