/**
 * Executors service exports
 *
 * Infrastructure layer for executing tasks:
 * - CLI Executors: Local binary process execution (ripgrep, git, etc.)
 * - Remote Executors: Network service execution (layertwine gRPC, etc.)
 *
 * Note: Tool Executors (business logic layer) have been moved to services/tools/
 */

// ============================================================================
// CLI/Local Binary Executor (subprocess-based execution)
// ============================================================================
export * from "./cli/index.js";

// ============================================================================
// Remote Service Executor (network-based execution)
// ============================================================================
export * from "./remote/index.js";

// ============================================================================
// MCP Server Executor (MCP protocol-based execution)
// ============================================================================
export * from "./mcp/index.js";

