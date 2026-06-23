/**
 * MCP Executor Module
 *
 * Provides MCP server execution capabilities following the BaseRemoteExecutor pattern.
 * Includes:
 * - McpServerExecutor: Single server connection management
 * - McpExecutorFactory: Connection pooling and lifecycle management
 * - McpConnectionManager: Low-level connection and RPC handling
 */

// Executor classes
export { McpServerExecutor } from "./mcp-server-executor.js";
export { McpExecutorFactory } from "./mcp-executor-factory.js";

// Core connection management
export { McpConnectionManager } from "./core/index.js";
export { McpClient } from "./core/index.js";
export { McpServerRegistry, getMcpManager, releaseMcpManager } from "./core/index.js";

// State management
export {
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
  isConnected,
  isDisabled,
  getServerDisplayName,
  updateLastActivity,
  updateLastHealthCheck,
  isIdleBeyond,
} from "./core/index.js";

// Features - Context Provider
export { McpToolsDynamicContextProvider, createMcpToolsContextProvider } from "./features/metadata/dynamic-context-provider.js";
export type { McpToolsContextOptions, GeneratedMcpToolsContext } from "./features/metadata/dynamic-context-provider.js";

// Features - Metadata Exporter
export { McpToolMetadataExporter } from "./features/metadata/tool-metadata-exporter.js";
export type { McpToolInfo, McpServerMetadata, ExportedMcpToolsContext } from "./features/metadata/tool-metadata-exporter.js";

// Features - Metadata Cache
export { McpToolMetadataCache } from "./features/metadata/metadata-cache.js";
export type { McpToolMetadataCacheConfig } from "./features/metadata/metadata-cache.js";

// Features - Registration
export { McpToolsRegistrar, createMcpToolsRegistrar } from "./features/registration/dynamic-registrar.js";
export type { McpToolRegistrationOptions } from "./features/registration/dynamic-registrar.js";

// Features - Approval
export { EnhancedMcpApprovalSystem } from "./features/approval/index.js";
export type { ParameterApprovalRule, RateLimitingRule, AccessControlRule, ToolCallApprovalContext, ResourceAccessApprovalContext } from "./features/approval/index.js";

// Features - Analytics
export { McpToolsUsageAnalytics } from "./features/analytics/usage-analytics.js";
export type { ToolExecutionStats, ToolAnalyticsEntry, AnalyticsReport } from "./features/analytics/usage-analytics.js";

// Types - Server Configuration
export type {
  McpServerConfigBase,
  McpStdioConfig,
  McpSseConfig,
  McpStreamableHttpConfig,
  McpServerConfig,
} from "./types.js";

// Types - Server State
export type { McpServerState, McpServerStatus } from "./types.js";

// Types - Tools & Resources
export type {
  McpTool,
  McpResource,
  McpResourceTemplate,
  McpToolCallResult,
  McpResourceReadResult,
} from "./types.js";

// Types - Settings & Configuration
export type {
  McpManagerOptions,
  McpEventHandler,
  McpEventType,
  McpServerSource,
  McpServerLifecycle,
  McpSettings,
  McpConnectionState,
  McpErrorEntry,
  McpHealthCheckStrategy,
} from "./types.js";

// Transport
export { createTransport } from "./transport/index.js";
export {
  StdioTransport,
  SseTransport,
  StreamableHttpTransport,
  isTransportTypeSupported,
} from "./transport/index.js";
export type { IMcpTransport, TransportConfig, TransportEventHandlers, TransportOptions } from "./transport/types.js";
export type { McpTransportType } from "./types.js";

// Configuration Processing
export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
  resolveServerLifecycle,
} from "./mcp-connection-processor.js";
export type { ResolvedLifecycle } from "./mcp-connection-processor.js";
