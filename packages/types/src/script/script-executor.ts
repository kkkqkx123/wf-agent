/**
 * Script Executor Configuration Types
 * Defines executor modes and shell type configuration for script execution
 */

/**
 * Shell type enumeration
 * Determines which shell interpreter to use for executing commands
 */
export type ShellType = "powershell" | "bash" | "cmd" | "auto";

/**
 * Executor mode enumeration
 * Determines how the script command is executed against the terminal
 * - Standard modes (existing behavior)
 * - Sandbox modes (new, requires sandbox config)
 */
export type ExecutorMode =
  | "direct"
  | "shared"
  | "pty"
  | "sandbox-shell"
  | "sandbox-python"
  | "sandbox-javascript";

/**
 * Script Executor Configuration
 * Configures how a script should be executed
 */
export interface ScriptExecutorConfig {
  /** Executor mode (direct/shared/pty) */
  mode: ExecutorMode;
  /** Target shell type */
  shell: ShellType;
  /** Working directory for execution */
  cwd?: string;
  /** Environment variables to set */
  environment?: Record<string, string>;
}