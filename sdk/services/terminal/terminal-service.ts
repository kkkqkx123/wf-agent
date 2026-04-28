/**
 * Terminal Service
 * 
 * Provides unified terminal session management with:
 * - Multi-shell support (bash, zsh, fish, pwsh, cmd, powershell, git-bash, wsl)
 * - Working directory management
 * - Environment variable maintenance
 * - Session reuse and lifecycle management
 * - Integration with auto-approval service
 */

import { spawn, ChildProcess } from "child_process";
import { EventEmitter } from "events";
import { cwd as processCwd } from "process";
import { ShellDetector, shellDetector } from "./shell-detector.js";
import { TerminalRegistry, terminalRegistry } from "./terminal-registry.js";
import type {
  ShellType,
  TerminalSession,
  TerminalSessionOptions,
  ExecuteOptions,
  ExecuteResult,
  OutputOptions,
  TerminalServiceConfig,
  TerminalServiceEvents,
} from "./types.js";

/**
 * Terminal Service
 * 
 * Main service class for terminal session management.
 */
export class TerminalService extends EventEmitter<TerminalServiceEvents> {
  private config: TerminalServiceConfig;
  private shellDetector: ShellDetector;
  private registry: TerminalRegistry;
  private processes: Map<string, ChildProcess> = new Map();

  constructor(config?: TerminalServiceConfig) {
    super();
    this.config = config ?? {};
    this.shellDetector = shellDetector;
    this.registry = terminalRegistry;
  }

  /**
   * Create a new terminal session
   */
  async createSession(options?: TerminalSessionOptions): Promise<TerminalSession> {
    // Resolve shell type
    const shellType = await this.shellDetector.resolveShellType(
      options?.shellType ?? this.config.defaultShellType
    );

    // Resolve working directory
    const sessionCwd = options?.cwd ?? this.config.defaultCwd ?? processCwd();

    // Merge environment variables
    const sessionEnv = {
      ...this.config.defaultEnv,
      ...options?.env,
    };

    // Create session
    const session = this.registry.createSession(shellType, sessionCwd, {
      ...options,
      env: sessionEnv,
    });

    // Emit event
    this.emit("session:created", session);

    return session;
  }

  /**
   * Get an existing session
   */
  getSession(sessionId: string): TerminalSession | undefined {
    return this.registry.get(sessionId);
  }

  /**
   * Get or create a session for a specific working directory
   * 
   * Implements intelligent session reuse.
   */
  async getOrCreateSession(
    cwd: string,
    options?: TerminalSessionOptions
  ): Promise<TerminalSession> {
    // Try to find an available session
    const existingSession = this.registry.findAvailable(cwd, options?.taskId);

    if (existingSession) {
      // Update task ID if provided
      if (options?.taskId) {
        this.registry.updateTaskId(existingSession.sessionId, options.taskId);
      }
      return existingSession;
    }

    // Create new session
    return this.createSession({
      ...options,
      cwd,
    });
  }

  /**
   * Execute a command in a session
   */
  async executeInSession(
    sessionId: string,
    command: string,
    options?: ExecuteOptions
  ): Promise<ExecuteResult> {
    const session = this.registry.get(sessionId);

    if (!session) {
      return {
        success: false,
        stdout: "",
        stderr: "",
        exitCode: -1,
        error: `Session not found: ${sessionId}`,
      };
    }

    // Update session status
    this.registry.updateStatus(sessionId, "busy");

    try {
      // Execute command
      const result = await this.executeCommand(
        command,
        session.shellType,
        options?.cwd ?? session.cwd,
        { ...session.env, ...options?.env },
        options?.timeout ?? this.config.defaultTimeout
      );

      // Update session activity
      session.lastActiveAt = Date.now();

      // Emit event
      this.emit("command:completed", sessionId, result);

      return {
        ...result,
        sessionId,
      };
    } catch (error) {
      const result: ExecuteResult = {
        success: false,
        stdout: "",
        stderr: error instanceof Error ? error.message : String(error),
        exitCode: -1,
        error: error instanceof Error ? error.message : String(error),
        sessionId,
      };

      this.emit("error", error instanceof Error ? error : new Error(String(error)), sessionId);

      return result;
    } finally {
      // Reset session status
      this.registry.updateStatus(sessionId, "idle");
    }
  }

  /**
   * Execute a one-off command (stateless)
   */
  async executeOneOff(
    command: string,
    options?: TerminalSessionOptions & ExecuteOptions
  ): Promise<ExecuteResult> {
    // Resolve shell type
    const shellType = await this.shellDetector.resolveShellType(
      options?.shellType ?? this.config.defaultShellType
    );

    // Resolve working directory
    const execCwd = options?.cwd ?? this.config.defaultCwd ?? processCwd();

    // Merge environment variables
    const execEnv = {
      ...this.config.defaultEnv,
      ...options?.env,
    };

    // Execute command
    return this.executeCommand(
      command,
      shellType,
      execCwd,
      execEnv,
      options?.timeout ?? this.config.defaultTimeout
    );
  }

  /**
   * Get output from a session
   */
  async getOutput(
    sessionId: string,
    options?: OutputOptions
  ): Promise<string> {
    const session = this.registry.get(sessionId);

    if (!session) {
      return "";
    }

    // Get output lines
    const fromIndex = options?.all ? 0 : undefined;
    let lines = this.registry.getOutput(sessionId, fromIndex);

    // Apply filter if provided
    if (options?.filter) {
      try {
        const regex = new RegExp(options.filter);
        lines = lines.filter((line) => regex.test(line));
      } catch {
        // Invalid regex, return all lines
      }
    }

    // Mark as read if not getting all output
    if (!options?.all) {
      this.registry.markOutputRead(sessionId);
    }

    return lines.join("\n");
  }

  /**
   * Terminate a session
   */
  async terminateSession(sessionId: string): Promise<ExecuteResult> {
    const session = this.registry.get(sessionId);

    if (!session) {
      return {
        success: false,
        stdout: "",
        stderr: "",
        exitCode: -1,
        error: `Session not found: ${sessionId}`,
      };
    }

    // Get remaining output
    const remainingOutput = await this.getOutput(sessionId);

    // Terminate process if running
    const process = this.processes.get(sessionId);
    if (process?.pid) {
      try {
        process.kill("SIGTERM");

        // Wait for graceful termination
        await new Promise<void>((resolve) => {
          setTimeout(() => {
            try {
              process.kill("SIGKILL");
            } catch {
              // Process may have already terminated
            }
            resolve();
          }, 5000);
        });
      } catch {
        // Process may have already terminated
      }
    }

    // Remove process
    this.processes.delete(sessionId);

    // Terminate session
    this.registry.terminate(sessionId);

    // Emit event
    this.emit("session:terminated", sessionId);

    return {
      success: true,
      stdout: remainingOutput,
      stderr: "",
      exitCode: 0,
      sessionId,
    };
  }

  /**
   * Release sessions associated with a task
   */
  releaseSessionsForTask(taskId: string): void {
    this.registry.releaseForTask(taskId);
  }

  /**
   * Cleanup all sessions
   */
  async cleanup(): Promise<void> {
    // Terminate all processes
    for (const [sessionId, process] of this.processes) {
      try {
        process.kill("SIGTERM");
      } catch {
        // Ignore errors
      }
    }

    this.processes.clear();
    this.registry.cleanup();
  }

  /**
   * Get all sessions
   */
  getAllSessions(): TerminalSession[] {
    return this.registry.getAll();
  }

  /**
   * Get session count
   */
  getSessionCount(): number {
    return this.registry.getCount();
  }

  /**
   * Execute a command using child_process
   */
  private async executeCommand(
    command: string,
    shellType: ShellType,
    cwd: string,
    env: Record<string, string>,
    timeout?: number
  ): Promise<ExecuteResult> {
    return new Promise((resolve) => {
      let stdout = "";
      let stderr = "";

      // Get shell path and args
      const shellPath = this.shellDetector.getShellPath(shellType);
      const shellArgs = this.shellDetector.getShellArgs(shellType, command);

      // Merge environment with process environment
      const mergedEnv = {
        ...process.env,
        ...env,
      };

      // Spawn process
      const proc = spawn(shellPath, shellArgs, {
        cwd,
        env: mergedEnv,
        windowsHide: true,
      });

      // Set up timeout
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      if (timeout) {
        timeoutId = setTimeout(() => {
          proc.kill("SIGKILL");
          resolve({
            success: false,
            stdout,
            stderr: stderr + "\nCommand timed out",
            exitCode: -1,
            error: "Command timed out",
          });
        }, timeout);
      }

      // Handle stdout
      proc.stdout?.on("data", (data: Buffer) => {
        const chunk = data.toString();
        stdout += chunk;

        // Emit output event for each line
        const lines = chunk.split("\n");
        for (const line of lines) {
          if (line.trim()) {
            this.emit("output:received", "", line);
          }
        }
      });

      // Handle stderr
      proc.stderr?.on("data", (data: Buffer) => {
        stderr += data.toString();
      });

      // Handle completion
      proc.on("close", (code) => {
        if (timeoutId) {
          clearTimeout(timeoutId);
        }

        const exitCode = code ?? 0;
        const success = exitCode === 0;

        resolve({
          success,
          stdout,
          stderr,
          exitCode,
          error: success ? undefined : `Command failed with exit code ${exitCode}`,
        });
      });

      // Handle error
      proc.on("error", (error) => {
        if (timeoutId) {
          clearTimeout(timeoutId);
        }

        resolve({
          success: false,
          stdout,
          stderr: stderr + "\n" + error.message,
          exitCode: -1,
          error: error.message,
        });
      });
    });
  }

  /**
   * Start a background command in a session
   */
  async startBackgroundCommand(
    sessionId: string,
    command: string
  ): Promise<ExecuteResult> {
    const session = this.registry.get(sessionId);

    if (!session) {
      return {
        success: false,
        stdout: "",
        stderr: "",
        exitCode: -1,
        error: `Session not found: ${sessionId}`,
      };
    }

    // Update session status
    this.registry.updateStatus(sessionId, "busy");

    // Get shell path and args
    const shellPath = this.shellDetector.getShellPath(session.shellType);
    const shellArgs = this.shellDetector.getShellArgs(session.shellType, command);

    // Merge environment
    const mergedEnv = {
      ...process.env,
      ...session.env,
    };

    // Spawn process
    const proc = spawn(shellPath, shellArgs, {
      cwd: session.cwd,
      env: mergedEnv,
      windowsHide: true,
    });

    // Store process
    this.processes.set(sessionId, proc);

    // Handle output
    const handleOutput = (data: Buffer) => {
      const lines = data.toString().split("\n");
      for (const line of lines) {
        if (line.trim()) {
          this.registry.addOutput(sessionId, line);
          this.emit("output:received", sessionId, line);
        }
      }
    };

    proc.stdout?.on("data", handleOutput);
    proc.stderr?.on("data", handleOutput);

    // Handle completion
    proc.on("close", (code) => {
      this.registry.updateStatus(sessionId, "idle");
      this.processes.delete(sessionId);

      const result: ExecuteResult = {
        success: code === 0,
        stdout: "",
        stderr: "",
        exitCode: code ?? 0,
        sessionId,
      };

      this.emit("command:completed", sessionId, result);
    });

    // Handle error
    proc.on("error", (error) => {
      this.registry.updateStatus(sessionId, "idle");
      this.processes.delete(sessionId);
      this.registry.addOutput(sessionId, `Process error: ${error.message}`);
      this.emit("error", error, sessionId);
    });

    // Emit event
    this.emit("command:started", sessionId, command);

    return {
      success: true,
      stdout: `Background command started in session ${sessionId}`,
      stderr: "",
      exitCode: 0,
      sessionId,
    };
  }

  /**
   * Kill background command in a session
   */
  async killBackgroundCommand(sessionId: string): Promise<ExecuteResult> {
    const process = this.processes.get(sessionId);

    if (!process) {
      return {
        success: false,
        stdout: "",
        stderr: "",
        exitCode: -1,
        error: `No running process in session: ${sessionId}`,
        sessionId,
      };
    }

    // Get remaining output
    const remainingOutput = await this.getOutput(sessionId);

    // Terminate process
    try {
      process.kill("SIGTERM");

      // Wait for graceful termination
      await new Promise<void>((resolve) => {
        setTimeout(() => {
          try {
            process.kill("SIGKILL");
          } catch {
            // Process may have already terminated
          }
          resolve();
        }, 5000);
      });
    } catch {
      // Process may have already terminated
    }

    // Remove process
    this.processes.delete(sessionId);

    // Update session status
    this.registry.updateStatus(sessionId, "idle");

    return {
      success: true,
      stdout: remainingOutput,
      stderr: "",
      exitCode: process.exitCode ?? 0,
      sessionId,
    };
  }
}

/**
 * Default terminal service instance
 */
let defaultTerminalService: TerminalService | undefined;

/**
 * Get the default terminal service instance
 */
export function getTerminalService(config?: TerminalServiceConfig): TerminalService {
  if (!defaultTerminalService) {
    defaultTerminalService = new TerminalService(config);
  }
  return defaultTerminalService;
}

/**
 * Create a new terminal service instance
 */
export function createTerminalService(config?: TerminalServiceConfig): TerminalService {
  return new TerminalService(config);
}
