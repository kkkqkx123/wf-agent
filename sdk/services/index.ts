/**
 * Core Services - Unified Export
 * 
 * This module provides unified access to all core services:
 * - Auto Approval Service: Automatic approval checking for tool execution
 * - Ignore Service: File/directory ignore pattern matching
 * - Protect Service: File write protection control
 * - Terminal Service: Shell session management and command execution
 */

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
} from './auto-approval/index.js';

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
  ServerConfigSchema,
  McpSettingsSchema,
  validateServerConfig,
  validateMcpSettings,
  isStdioConfig,
  isSseConfig,
  isStreamableHttpConfig,
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
