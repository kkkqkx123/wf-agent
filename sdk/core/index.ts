// Core module exports

// Checkpoint
export * from "./utils/checkpoint/index.js";

// Coordinators
export * from "./coordinators/index.js";

// DI
export * from "./di/index.js";

// Execution
export * from "./execution/index.js";

// Executors
export * from "./executors/index.js";

// Hooks
export * from "./hooks/index.js";

// LLM
export * from "./llm/index.js";

// Messaging
export * from "./messaging/index.js";

// Prompt
export * from "./prompt/index.js";

// Registry
export * from "./registry/index.js";

// Services (re-exported from sdk/services)
export {
  checkAutoApproval,
  extractContextFromParameters,
  type AutoApprovalDecision,
  type AutoApprovalContext,
  type CheckAutoApprovalParams,
  containsDangerousSubstitution,
  findLongestPrefixMatch,
  getCommandDecision,
  getSingleCommandDecision,
  type CommandDecision,
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
} from "../services/auto-approval/index.js";

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
} from "../services/terminal/index.js";

// Triggers
export * from "./triggers/index.js";

// Types
export * from "./types/index.js";

// Utils
export * from "./utils/index.js";

// Validation
export * from "./validation/index.js";
