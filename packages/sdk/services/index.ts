/**
 * Core Services - Unified Export
 *
 * This module provides unified access to all core services:
 * - Evaluation Service: Condition evaluation with DSL support (expression, predicate, schema, script)
 * - Skill Loader Service: File I/O abstraction for skill loading
 * - Auto Approval Service: Automatic approval checking for tool execution
 * - Ignore Service: File/directory ignore pattern matching
 * - Protect Service: File write protection control
 * - Terminal Service: Shell session management and command execution
 * - Shutdown Service: Graceful shutdown management for SDK lifecycle
 */

// ============================================================================
// Evaluation Service (Condition Evaluation with DSL Support)
// ============================================================================
export {
  ConditionEvaluator,
  conditionEvaluator,
  CacheManager,
  cacheManager,
  BaseExecutor,
  DependencyManager,
  createDependencyManager,
  expressionEvaluator,
  validateExpression,
  validatePath,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
  resolvePath,
  pathExists,
  setPath,
  setArrayItemByKey,
  dslParse,
  dslParseWithErrors,
  dslValidate,
  parseToCst,
  cstToAst,
  tokenizeExpression,
  type CompiledUnit,
  type ICompiler,
  type IExecutor,
  type Expression,
  type LiteralExpr,
  type IdentifierExpr,
  type MemberAccessExpr,
  type UnaryMinusExpr,
  type BinaryExpr,
  type NotExpr,
  type TernaryExpr,
  type CallExpr,
  type ArrayLiteralExpr,
  type NodeMetadata,
  type BinaryOperator,
  type EvaluationContext,
} from "./evaluation/index.js";

// ============================================================================
// Skill Loader Service
// ============================================================================
export {
  HostSkillLoader,
  type SkillFileLoader,
  type SkillDirectoryEntry,
} from "./skill-loader/index.js";

// ============================================================================
// Ignore Service
// ============================================================================
export {
  IgnoreController,
  IgnoreMode,
  MAX_FILE_RESULTS,
  type IgnoreControllerConfig,
} from "./ignore/index.js";

// ============================================================================
// Protect Service
// ============================================================================
export { ProtectController, SHIELD_SYMBOL, type ProtectControllerConfig } from "./protect/index.js";

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
} from "./terminal/index.js";

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
} from "./auto-approval/index.js";

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
} from "./command-safety/index.js";

// ============================================================================
// Transport Layer Service
// ============================================================================
export {
  // Types
  type TransportProtocol,
  type TransportConnectionConfig,
  type TransportCallResult,
  type TransportError,
  type ITransportClient,
  type CallOptions,
  // gRPC
  GrpcClient,
  GrpcClientManager,
  GrpcHealthCheck,
  type GrpcClientOptions,
  type GrpcClientState,
  type GrpcHealthCheckConfig,
} from "./transport/index.js";

// ============================================================================
// Executors Service (CLI/Local Binary Executors)
// ============================================================================
export {
  BaseCliExecutor,
  type ExecutorConfig,
  type ExecutorInfo,
  type ExecutorStatus,
  type ExecutionOptions,
  type ExecutionResult,
} from "./executors/cli/index.js";

// CLI Executor (Local Binary Execution)
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
} from "./executors/cli/implementations/ripgrep/index.js";

// Remote Service Executor (Network Services)
export {
  // Base class
  BaseRemoteExecutor,
  // Types
  type RemoteConnectionConfig,
  type RemoteExecutorStatus,
  type RemoteExecutionResult,
  type RemoteExecutorConfig,
  // Layertwine gRPC Executor
  LayertwineExecutor,
  LayertwineProcessManager,
  type LayertwineDeployMode,
  type LayertwineExecutorConfig,
  type LayertwineInitRequest,
  type LayertwineInitResponse,
  type LayertwineEditRequest,
  type LayertwineEditResponse,
  type LayertwineStatusResponse,
  type LayertwinePartitionInfo,
  type LayertwineCommitRequest,
  type LayertwineCommitResponse,
  type LayertwineLogRequest,
  type LayertwineLogResponse,
  type LayertwineCheckpointInfo,
  type LayertwineBranchListResponse,
  type LayertwineBranchInfo,
  type LayertwineAgentEditRequest,
  type LayertwineAgentEditResponse,
  type LayertwineAgentSubmitRequest,
  type LayertwineAgentSubmitResponse,
  type LayertwineApproveRequest,
  type LayertwineApproveResponse,
  type LayertwineBackupRequest,
  type LayertwineBackupResponse,
} from "./executors/remote/index.js";

// ============================================================================
// Tool Executors (Moved to services/tools/)
// ============================================================================
export {
  // Core interfaces and base classes
  IToolExecutor,
  BaseExecutor as ToolBaseExecutor,
  ParameterValidator,
  RetryStrategy,
  TimeoutController,
  type ToolType,
  type ExecutorMetadata,
} from "./tools/index.js";

// REST Executor
export {
  RestExecutor,
  type HttpRequestConfig,
  type HttpResponse,
  type RequestInterceptor,
  type ResponseInterceptor,
  type ErrorInterceptor,
  type RestExecutorConfig,
} from "./tools/index.js";

// Stateful Executor
export { StatefulExecutor, type StatefulExecutorConfig } from "./tools/index.js";

// Stateless Executor
export {
  StatelessExecutor,
  type FunctionRegistryItem,
  type FunctionRegistryConfig,
} from "./tools/index.js";

// Builtin Executor
export { BuiltinExecutor, type BuiltinExecutorConfig } from "./tools/index.js";

// Utility functions
export { toSdkTool, toSdkTools, type ToolDefinitionLike } from "./tools/index.js";


// ============================================================================
// MCP Service
// ============================================================================
export {
  // Executor classes
  McpServerExecutor,
  McpExecutorFactory,
  // Types - Server Configuration
  type McpServerConfigBase,
  type McpStdioConfig,
  type McpSseConfig,
  type McpStreamableHttpConfig,
  type McpServerConfig,
  // Types - Server State
  type McpServerState,
  type McpServerStatus,
  // Types - Tools & Resources
  type McpTool,
  type McpResource,
  type McpResourceTemplate,
  type McpToolCallResult,
  type McpResourceReadResult,
  // Types - Settings & Configuration
  type McpManagerOptions,
  type McpEventHandler,
  type McpEventType,
  type McpServerSource,
  type McpServerLifecycle,
  type McpSettings,
  type McpConnectionState,
  type McpErrorEntry,
  // Connection Management
  McpConnectionManager,
  McpServerRegistry,
  getMcpManager,
  releaseMcpManager,
  McpClient,
  // State utilities
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
  isConnected,
  isDisabled,
  getServerDisplayName,
  // Features
  McpToolsDynamicContextProvider,
  createMcpToolsContextProvider,
  McpToolMetadataExporter,
  McpToolsRegistrar,
  createMcpToolsRegistrar,
  EnhancedMcpApprovalSystem,
  McpToolsUsageAnalytics,
  // Transport
  createTransport,
  type IMcpTransport,
  type TransportConfig,
  type TransportEventHandlers,
  type TransportOptions,
  type McpTransportType,
  // Config processing
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
} from "./executors/mcp/index.js";

// ============================================================================
// Shutdown Service
// ============================================================================
export {
  GracefulShutdownManager,
  type GracefulShutdownConfig,
  type ShutdownSignal,
  type ShutdownCheckpointResult,
} from "./shutdown/index.js";

// ============================================================================
// HTTP Service (Transport Layer)
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
} from "./transport/http/index.js";

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
} from "./sandbox/index.js";

// ============================================================================
// Script Service (Execution Engine)
// ============================================================================
export {
  ScriptEngine,
  type ScriptEngineOptions,
  ScriptFlowEngine,
  type FlowExecutionResult,
  type BranchExecutionResult,
  ScriptTemplateEngine,
} from "./script/index.js";

// ============================================================================
// VFS Service
// ============================================================================
export { SandboxVFS, type VFSEntry, type VFSOperations } from "./vfs/index.js";
