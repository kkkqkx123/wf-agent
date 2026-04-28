/**
 * Terminal Service
 * 
 * Unified export for terminal service functionality.
 */

// Types
export type {
  ShellType,
  SessionStatus,
  TerminalSessionOptions,
  TerminalSession,
  ExecuteOptions,
  ExecuteResult,
  OutputOptions,
  TerminalServiceConfig,
  ShellInfo,
  ProcessInfo,
  TerminalSessionWithProcess,
  TerminalServiceEvents,
} from "./types.js";

// Shell Detector
export { ShellDetector, shellDetector } from "./shell-detector.js";

// Terminal Registry
export { TerminalRegistry, terminalRegistry } from "./terminal-registry.js";

// Terminal Service
export {
  TerminalService,
  getTerminalService,
  createTerminalService,
} from "./terminal-service.js";
