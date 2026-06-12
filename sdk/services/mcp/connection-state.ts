/**
 * MCP Connection State Management
 * Handles connection state tracking and transitions
 */

import type { McpServerState, McpServerStatus, McpServerSource, McpErrorEntry } from "./types.js";

/**
 * Create initial server state
 */
export function createInitialServerState(
  name: string,
  config: string,
  source: McpServerSource,
  projectPath?: string,
): McpServerState {
  return {
    name,
    config,
    status: "disconnected",
    disabled: false,
    source,
    projectPath,
    errorHistory: [],
    lastActivity: undefined,
    lastHealthCheck: undefined,
  };
}

/**
 * Update server status
 */
export function updateServerStatus(state: McpServerState, status: McpServerStatus): McpServerState {
  return {
    ...state,
    status,
  };
}

/**
 * Add error to history
 */
export function addErrorToHistory(
  state: McpServerState,
  message: string,
  level: "error" | "warn" | "info" = "error",
  maxHistory = 100,
): McpServerState {
  const errorEntry: McpErrorEntry = {
    message: truncateMessage(message, 1000),
    timestamp: Date.now(),
    level,
  };

  const errorHistory = [...state.errorHistory, errorEntry].slice(-maxHistory);

  return {
    ...state,
    error: errorEntry.message,
    errorHistory,
  };
}

/**
 * Clear error state
 */
export function clearErrorState(state: McpServerState): McpServerState {
  return {
    ...state,
    error: undefined,
  };
}

/**
 * Truncate message to max length
 */
function truncateMessage(message: string, maxLength: number): string {
  if (message.length <= maxLength) {
    return message;
  }
  return `${message.substring(0, maxLength)}...(truncated)`;
}

/**
 * Check if server is in a connectable state
 */
export function isConnectable(state: McpServerState): boolean {
  return !state.disabled && state.status !== "connecting";
}

/**
 * Check if server is connected
 */
export function isConnected(state: McpServerState): boolean {
  return state.status === "connected";
}

/**
 * Check if server is disabled
 */
export function isDisabled(state: McpServerState): boolean {
  return state.disabled === true;
}

/**
 * Get server display name (with source indicator)
 */
export function getServerDisplayName(state: McpServerState): string {
  const sourceIndicator = state.source === "project" ? " (project)" : "";
  return `${state.name}${sourceIndicator}`;
}

/**
 * Update last activity timestamp (used for idle timeout tracking)
 */
export function updateLastActivity(state: McpServerState): McpServerState {
  return {
    ...state,
    lastActivity: Date.now(),
  };
}

/**
 * Update last health check timestamp
 */
export function updateLastHealthCheck(state: McpServerState): McpServerState {
  return {
    ...state,
    lastHealthCheck: Date.now(),
  };
}

/**
 * Check if server has been idle beyond the specified timeout
 */
export function isIdleBeyond(state: McpServerState, idleTimeoutMs: number): boolean {
  if (idleTimeoutMs <= 0 || state.lastActivity === undefined) {
    return false;
  }
  return Date.now() - state.lastActivity > idleTimeoutMs;
}
