/**
 * Core Services - Unified Export
 * 
 * This module provides unified access to all core services:
 * - Skill Loader Service: File I/O abstraction for skill loading
 * - Auto Approval Service: Automatic approval checking for tool execution
 * - Ignore Service: File/directory ignore pattern matching
 * - Protect Service: File write protection control
 * - Terminal Service: Shell session management and command execution
 * - Shutdown Service: Graceful shutdown management for SDK lifecycle
 */

// ============================================================================
// Skill Loader Service
// ============================================================================
export {
  HostSkillLoader,
  type SkillFileLoader,
  type SkillDirectoryEntry,
} from './skill-loader/index.js';

// ============================================================================
// Ignore Service
// ============================================================================
export {
  IgnoreController,
  IgnoreMode,
  MAX_FILE_RESULTS,
  type IgnoreControllerConfig,
} from './ignore/index.js';

// ============================================================================
// Protect Service
// ============================================================================
export {
  ProtectController,
  SHIELD_SYMBOL,
  type ProtectControllerConfig,
} from './protect/index.js';

// ============================================================================
// Terminal Service
// ============================================================================
export {
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
  type ExecuteOptions,
  type ExecuteResult,
  type OutputOptions,
  type TerminalServiceConfig,
  type ShellInfo,
  type ProcessInfo,
  type TerminalSessionWithProcess,
  type TerminalServiceEvents,
} from './terminal/index.js';

// ============================================================================
// Auto Approval Service
// ============================================================================
export {
  checkAutoApproval,
  extractContextFromParameters,
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
} from './auto-approval/index.js';

// ============================================================================
// Command Safety — Shared Utilities
// ============================================================================
export {
  containsDangerousSubstitution,
  findLongestPrefixMatch,
  getCommandDecision,
  getSingleCommandDecision,
  parseCommandChain,
  type CommandDecision,
} from './command-safety/index.js';

// ============================================================================
// Executors Service
// ============================================================================
export {
  BaseExecutor,
  type ExecutorConfig,
  type ExecutorInfo,
  type ExecutorStatus,
  type ExecutionOptions,
  type ExecutionResult,
} from './executors/index.js';

// Ripgrep Executor
export {
  RipgrepExecutor,
  truncateLine,
  type RipgrepSearchOptions,
  type RipgrepListFilesOptions,
  type SearchFileResult,
  type SearchResult,
  type SearchLineResult,
  type FileResult,
} from './executors/implementations/ripgrep/index.js';

// Tool Executors
export {
  // Core interfaces and base classes
  IToolExecutor,
  ToolBaseExecutor,
  ParameterValidator,
  RetryStrategy,
  TimeoutController,
  type ExecutorMetadata,
} from './executors/index.js';

// REST Executor
export {
  RestExecutor,
  type HttpRequestConfig,
  type HttpResponse,
  type RequestInterceptor,
  type ResponseInterceptor,
  type ErrorInterceptor,
  type RestExecutorConfig,
} from './executors/index.js';

// Stateful Executor
export {
  StatefulExecutor,
  type StatefulExecutorConfig,
} from './executors/index.js';

// Stateless Executor
export {
  StatelessExecutor,
  FunctionRegistry,
  type FunctionRegistryItem,
  type FunctionRegistryConfig,
} from './executors/index.js';

// Builtin Executor
export {
  BuiltinExecutor,
  type BuiltinExecutorConfig,
} from './executors/index.js';

// Utility functions
export {
  toSdkTool,
  toSdkTools,
  type ToolDefinitionLike,
} from './executors/index.js';

// ============================================================================
// Search Service
// ============================================================================
export {
  SearchService,
  fuzzyMatch,
  sortByFuzzyMatch,
  type FileSearchOptions,
  type ListAllFilesOptions,
  type FileSearchResult,
  type FuzzyMatchResult,
} from './search/index.js';

// ============================================================================
// MCP Service
// ============================================================================
export {
  // Types
  type McpServerStatus,
  type McpTransportType,
  type McpServerSource,
  type McpServerConfigBase,
  type McpStdioConfig,
  type McpSseConfig,
  type McpStreamableHttpConfig,
  type McpServerConfig,
  type McpTool,
  type McpResource,
  type McpResourceTemplate,
  type McpErrorEntry,
  type McpServerState,
  type McpToolCallResult,
  type McpResourceReadResult,
  type McpSettings,
  type McpConnectionState,
  type McpManagerOptions,
  type McpEventType,
  type McpEventHandler,
  // Config
  DEFAULT_MCP_SETTINGS_FILE,
  PROJECT_MCP_FILE,
  loadMcpSettings,
  loadServerConfigs,
  fileExists as mcpFileExists,
  getGlobalMcpSettingsPath,
  getProjectMcpPath,
  createDefaultMcpSettings,
  writeMcpSettings,
  ensureMcpSettingsFile,
  mergeServerConfigs,
  // Transport
  type IMcpTransport,
  type TransportConfig,
  type TransportEventHandlers,
  type TransportOptions,
  StdioTransport,
  SseTransport,
  StreamableHttpTransport,
  createTransport,
  isTransportTypeSupported,
  // Client
  McpClient,
  // Connection State
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
  isConnected,
  isDisabled,
  getServerDisplayName,
  // Connection Manager
  McpConnectionManager,
  // Server Registry
  McpServerRegistry,
  getMcpManager,
  releaseMcpManager,
} from './mcp/index.js';

// ============================================================================
// Shutdown Service
// ============================================================================
export {
  GracefulShutdownManager,
  type GracefulShutdownConfig,
  type ShutdownSignal,
  type ShutdownCheckpointResult,
} from './shutdown/index.js';

// ============================================================================
// HTTP Service
// ============================================================================
export {
  // HTTP Client
  HttpClient,
  // HTTP Transport (distinct from MCP transport)
  SseTransport as HttpSseTransport,
  // Retry
  executeWithRetry,
  NonRetryableStatusCode,
  type RetryConfig,
  // Circuit Breaker (re-exported from core utils)
  CircuitBreaker,
  type CircuitBreakerConfig,
  // Rate Limiter
  RateLimiter,
  type RateLimiterConfig,
  // Interceptors
  RequestInterceptor as HttpRequestInterceptor,
  ResponseInterceptor as HttpResponseInterceptor,
  ErrorInterceptor as HttpErrorInterceptor,
  InterceptorManager,
  createAuthInterceptor,
  createLoggingInterceptor,
  createRetryInterceptor,
  // SSE Utilities
  parseSSELine,
  parseSSELines,
  streamSSE,
  readSSEStream,
  // HTTP Errors
  BadRequestError,
  UnauthorizedError,
  ForbiddenError,
  NotFoundHttpError,
  ConflictError,
  UnprocessableEntityError,
  RateLimitError,
  InternalServerError,
  ServiceUnavailableError,
  // Types
  type HTTPMethod,
  type HttpRequestOptions,
  type HttpResponse as HttpServiceResponse,
  type HttpClientConfig,
} from './http/index.js';

// ============================================================================
// Sandbox Service
// ============================================================================
export {
  SandboxRuntime,
  getSandboxRuntime,
  resetSandboxRuntime,
  DefaultStrategyResolver,
  DEFAULT_SANDBOX_POLICY,
  DEFAULT_SHELL_POLICY,
  DEFAULT_PYTHON_POLICY,
  DEFAULT_JS_POLICY,
  ShellStaticAnalyzerStrategy,
  PythonBuiltinHookStrategy,
  PythonASTAnalyzerStrategy,
  JavaScriptVmContextStrategy,
  type SandboxExecutionResult,
  type SandboxRuntimeResult,
} from './sandbox/index.js';

// ============================================================================
// VFS Service
// ============================================================================
export {
  SandboxVFS,
  type VFSEntry,
  type VFSOperations,
} from './vfs/index.js';
