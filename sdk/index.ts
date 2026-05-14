/**
 * Modular Agent SDK - Main Entry Point
 */

// Re-export API layer
export * from "./api/index.js";

// Re-export services layer (lifecycle management, shutdown, etc.)
// Note: Explicitly export to avoid naming conflicts with API layer
export {
  // Shutdown Service
  GracefulShutdownManager,
  type GracefulShutdownConfig,
  type ShutdownSignal,
  type ShutdownCheckpointResult,
  
  // Terminal Service
  TerminalService,
  getTerminalService,
  createTerminalService,
  ShellDetector,
  shellDetector,
  TerminalRegistry,
  terminalRegistry,
  type ShellType,
  type SessionStatus,
  type TerminalSessionOptions,
  type TerminalSession,
  type ExecuteOptions as TerminalExecuteOptions,
  type ExecuteResult as TerminalExecuteResult,
  type OutputOptions,
  type TerminalServiceConfig,
  type ShellInfo,
  type ProcessInfo,
  type TerminalSessionWithProcess,
  type TerminalServiceEvents,
  
  // Ignore Service
  IgnoreController,
  IgnoreMode,
  MAX_FILE_RESULTS,
  type IgnoreControllerConfig,
  
  // Protect Service
  ProtectController,
  SHIELD_SYMBOL,
  type ProtectControllerConfig,
  
  // Auto Approval Service
  checkAutoApproval,
  extractContextFromParameters,
  containsDangerousSubstitution,
  findLongestPrefixMatch,
  getCommandDecision,
  getSingleCommandDecision,
  checkFilePermission,
  matchesPattern,
  getEffectivePermission,
  batchCheckFilePermissions,
  createDefaultFilePermissionSettings,
  checkMcpApproval,
  createDefaultMcpApprovalSettings,
  mergeMcpApprovalSettings,
  isServerConfigured,
  getAutoApprovedTools,
  type AutoApprovalDecision,
  type AutoApprovalContext,
  type CheckAutoApprovalParams,
  type CommandDecision,
  
  // Search Service
  SearchService,
  fuzzyMatch,
  sortByFuzzyMatch,
  type FileSearchOptions,
  type ListAllFilesOptions,
  type FileSearchResult,
  type FuzzyMatchResult,
  
  // Tool Executors
  IToolExecutor,
  ToolBaseExecutor,
  ParameterValidator,
  RetryStrategy,
  TimeoutController,
  RestExecutor,
  StatefulExecutor,
  StatelessExecutor,
  FunctionRegistry,
  BuiltinExecutor,
  toSdkTool,
  toSdkTools,
  type ExecutorMetadata,
  type HttpRequestConfig,
  type HttpResponse,
  type RequestInterceptor,
  type ResponseInterceptor,
  type ErrorInterceptor,
  type RestExecutorConfig,
  type StatefulExecutorConfig,
  type FunctionRegistryItem,
  type FunctionRegistryConfig,
  type BuiltinExecutorConfig,
  type ToolDefinitionLike,
  
  // MCP Service (selected exports)
  McpClient,
  McpConnectionManager,
  McpServerRegistry,
  getMcpManager,
  releaseMcpManager,
  type McpServerStatus,
  type McpTransportType,
  type McpServerConfig,
  type McpTool,
  type McpResource,
  type McpSettings,
} from "./services/index.js";

// Re-export utilities
export * from "./utils/index.js";

// Re-export resources
export * from "./resources/index.js";

// Re-export interaction module
export { FollowupQuestionCoordinator } from "./core/coordinators/followup-question-coordinator.js";
export { ToolApprovalCoordinator } from "./core/coordinators/tool-approval-coordinator.js";
