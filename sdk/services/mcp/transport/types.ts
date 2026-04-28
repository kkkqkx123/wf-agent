/**
 * MCP Transport Layer Types
 * Abstract interfaces for MCP transport implementations
 */

import type { McpTransportType } from "../types.js";

/**
 * Transport connection options
 */
export interface TransportOptions {
  /** Connection timeout in milliseconds */
  timeout?: number;
  /** Retry attempts on connection failure */
  retryAttempts?: number;
  /** Retry delay in milliseconds */
  retryDelay?: number;
}

/**
 * Transport event handlers
 */
export interface TransportEventHandlers {
  /** Called when transport encounters an error */
  onError?: (error: Error) => void;
  /** Called when transport is closed */
  onClose?: () => void;
  /** Called when transport receives data */
  onData?: (data: unknown) => void;
}

/**
 * Abstract transport interface
 */
export interface IMcpTransport {
  /** Transport type */
  readonly type: McpTransportType;

  /** Whether the transport is connected */
  readonly isConnected: boolean;

  /**
   * Start the transport
   */
  start(): Promise<void>;

  /**
   * Close the transport
   */
  close(): Promise<void>;

  /**
   * Send a message through the transport
   */
  send(message: unknown): Promise<void>;

  /**
   * Set event handlers
   */
  setHandlers(handlers: TransportEventHandlers): void;
}

/**
 * Stdio transport configuration
 */
export interface StdioTransportConfig {
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
 * SSE transport configuration
 */
export interface SseTransportConfig {
  type: "sse";
  /** Server URL */
  url: string;
  /** HTTP headers */
  headers?: Record<string, string>;
}

/**
 * Streamable HTTP transport configuration
 */
export interface StreamableHttpTransportConfig {
  type: "streamable-http";
  /** Server URL */
  url: string;
  /** HTTP headers */
  headers?: Record<string, string>;
}

/**
 * Unified transport configuration
 */
export type TransportConfig =
  | StdioTransportConfig
  | SseTransportConfig
  | StreamableHttpTransportConfig;
