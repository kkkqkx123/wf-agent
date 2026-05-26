/**
 * Script Executor Configuration Types
 * Defines executor modes and shell type configuration for script execution
 */

// ============================================================================
// Shell Type System — layered hierarchy
// All derived from const arrays to keep types & schemas in sync.
// ============================================================================

/** All shell types the terminal service can work with */
export const SHELL_TYPES = [
  "bash",
  "zsh",
  "fish",
  "sh",
  "cmd",
  "powershell",
  "pwsh",
  "git-bash",
  "wsl",
] as const;

/** Shell types that support script execution (a subset of SHELL_TYPES) */
export const SCRIPT_SHELL_TYPES = ["bash", "powershell", "cmd"] as const;

/** Executor-level shell config values (script shells + auto-detect) */
export const EXECUTOR_SHELL_CONFIGS = [...SCRIPT_SHELL_TYPES, "auto"] as const;

/** All shell types the system can work with */
export type ShellType = (typeof SHELL_TYPES)[number];

/** Shell types that support script execution */
export type ScriptShellType = (typeof SCRIPT_SHELL_TYPES)[number];

/** Executor shell configuration (includes auto-detect) */
export type ExecutorShellConfig = (typeof EXECUTOR_SHELL_CONFIGS)[number];

// ============================================================================
// Executor Mode & Runtime
// ============================================================================

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
 * Execution runtime environment.
 *
 * Controls how/where the shell process is launched:
 *   - "native" — run the shell directly on the host OS
 *   - "wsl"    — run inside WSL (maps to bash analysis, executes via wsl.exe)
 *   - "docker" — run inside a Docker container (requires DockerConfig)
 *   - "ssh"    — run on a remote host via SSH (requires SSHConfig)
 *
 * This is orthogonal from shell type: e.g. shell=bash + runtime=docker
 * means "analyze as bash, execute via docker exec".
 */
export type ScriptRuntime = "native" | "wsl" | "docker" | "ssh";

// ============================================================================
// Runtime Connection Configuration
// ============================================================================

/** Docker runtime connection config */
export interface DockerConfig {
  /** Target container name or ID (required) */
  container: string;
  /** Docker host (e.g. "unix:///var/run/docker.sock", "tcp://...") */
  host?: string;
  /** TLS verify (default: false) */
  tlsVerify?: boolean;
  /** Run as a specific user inside the container */
  user?: string;
  /** Working directory inside the container */
  workdir?: string;
  /** Additional docker exec flags (e.g. ["-e", "MY_VAR=val"]) */
  extraFlags?: string[];
}

/** SSH runtime connection config */
export interface SSHConfig {
  /** Remote hostname or IP (required) */
  host: string;
  /** SSH port (default: 22) */
  port?: number;
  /** SSH user (default: current OS user) */
  user?: string;
  /** Path to SSH private key (optional, default: ~/.ssh/id_*) */
  identityFile?: string;
  /** SSH key passphrase (optional) */
  passphrase?: string;
  /** Password auth (optional, use only when key auth is unavailable) */
  password?: string;
  /** Forward agent (default: false) */
  forwardAgent?: boolean;
  /** Additional ssh flags */
  extraFlags?: string[];
}

/**
 * Union of all runtime-specific connection configs.
 * The discriminator is the ScriptRuntime value.
 */
export type RuntimeConfig = DockerConfig | SSHConfig;

// ============================================================================
// Script Executor Config
// ============================================================================

/**
 * Script Executor Configuration
 * Configures how a script should be executed
 */
export interface ScriptExecutorConfig {
  /** Executor mode (direct/shared/pty) */
  mode: ExecutorMode;
  /** Target shell type (use "auto" for platform default) */
  shell: ExecutorShellConfig;
  /** Execution runtime environment (default: "native") */
  runtime?: ScriptRuntime;
  /** Runtime-specific connection config (required when runtime is docker/ssh) */
  runtimeConfig?: RuntimeConfig;
  /** Working directory for execution */
  cwd?: string;
  /** Environment variables to set */
  environment?: Record<string, string>;
}
