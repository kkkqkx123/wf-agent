/**
 * Backend-shell tool handlers using Terminal Service
 *
 * Provides background shell execution with:
 * - Multi-shell support
 * - Working directory management
 * - Environment variable maintenance
 * - Session reuse
 */

import type { ShellType } from "@wf-agent/sdk/services";
import { getTerminalService } from "@wf-agent/sdk/services";
import type { BackendShell, ShellOutputResult } from "./types.js";

/**
 * Backend Shell Manager (Legacy compatibility layer)
 *
 * This class provides backward compatibility with the existing BackendShell interface
 * while using the new TerminalService internally.
 */
class BackendShellManager {
  private shells: Map<string, BackendShell> = new Map();

  /**
   * Add a backend Shell
   */
  add(shell: BackendShell): void {
    this.shells.set(shell.shellId, shell);
  }

  /**
   * Get the backend Shell
   */
  get(shellId: string): BackendShell | undefined {
    return this.shells.get(shellId);
  }

  /**
   * Get all available shell IDs
   */
  getAvailableIds(): string[] {
    return Array.from(this.shells.keys());
  }

  /**
   * Remove the backend Shell.
   */
  remove(shellId: string): boolean {
    return this.shells.delete(shellId);
  }

  /**
   * Start monitoring
   */
  startMonitor(shell: BackendShell): void {
    const proc = shell.process;

    const handleOutput = (data: Buffer) => {
      const lines = data.toString().split("\n");
      for (const line of lines) {
        if (line.trim()) {
          shell.outputLines.push(line);
        }
      }
    };

    proc.stdout?.on("data", handleOutput);
    proc.stderr?.on("data", handleOutput);

    proc.on("close", code => {
      shell.status = code === 0 ? "completed" : "failed";
      shell.exitCode = code;
    });

    proc.on("error", error => {
      shell.status = "error";
      shell.outputLines.push(`Process error: ${error.message}`);
    });
  }

  /**
   * Terminate the background Shell process.
   */
  async terminate(shellId: string): Promise<BackendShell> {
    const shell = this.shells.get(shellId);
    if (!shell) {
      throw new Error(`Shell not found: ${shellId}`);
    }

    // Terminate the process.
    if (shell.process.pid) {
      try {
        process.kill(shell.process.pid, "SIGTERM");
        // Wait for 5 seconds and then terminate forcibly.
        await new Promise<void>(resolve => {
          setTimeout(() => {
            try {
              process.kill(shell.process.pid!, "SIGKILL");
            } catch {
              // The process may have already ended.
            }
            resolve();
          }, 5000);
        });
      } catch {
        // The process may have already ended.
      }
    }

    shell.status = "terminated";
    shell.exitCode = shell.process.exitCode ?? -1;
    this.shells.delete(shellId);

    return shell;
  }

  /**
   * Clean up all Shell scripts.
   */
  cleanup(): void {
    for (const [, shell] of this.shells) {
      if (shell.process.pid) {
        try {
          process.kill(shell.process.pid, "SIGTERM");
        } catch {
          // Ignore errors
        }
      }
    }
    this.shells.clear();
  }
}

/**
 * Create a backend_shell tool factory
 */
export function createBackendShellFactory() {
  return () => {
    const manager = new BackendShellManager();
    const terminalService = getTerminalService();

    return {
      execute: async (params: Record<string, unknown>): Promise<ShellOutputResult> => {
        const { command, shell_type, cwd, env } = params as {
          command: string;
          shell_type?: ShellType;
          cwd?: string;
          env?: Record<string, string>;
        };

        try {
          // Create or get session with specified options
          const session = await terminalService.getOrCreateSession(cwd ?? process.cwd(), {
            shellType: shell_type,
            env: env,
          });

          // Start background command
          const result = await terminalService.startBackgroundCommand(session.sessionId, command);

          if (!result.success) {
            return {
              success: false,
              content: "",
              error: result.error,
              stdout: "",
              stderr: result.error ?? "",
              exitCode: -1,
            };
          }

          return {
            success: true,
            content: `Command started in background. Use shell_output to monitor (shell_id='${session.sessionId}').\n\nCommand: ${command}\nShell ID: ${session.sessionId}\nShell Type: ${session.shellType}\nWorking Directory: ${session.cwd}`,
            stdout: `Background command started with ID: ${session.sessionId}`,
            stderr: "",
            exitCode: 0,
            shellId: session.sessionId,
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: error instanceof Error ? error.message : String(error),
            stdout: "",
            stderr: error instanceof Error ? error.message : String(error),
            exitCode: -1,
          };
        }
      },
      cleanup: () => {
        manager.cleanup();
        terminalService.cleanup();
      },
    };
  };
}

/**
 * Create a shell_output tool factory
 */
export function createShellOutputFactory() {
  return () => {
    const terminalService = getTerminalService();

    return {
      execute: async (params: Record<string, unknown>): Promise<ShellOutputResult> => {
        const { shell_id, filter_str } = params as {
          shell_id: string;
          filter_str?: string;
        };

        try {
          // Get output from terminal service
          const output = await terminalService.getOutput(shell_id, {
            filter: filter_str,
          });

          const session = terminalService.getSession(shell_id);

          return {
            success: true,
            content: output || "(no new output)",
            stdout: output,
            stderr: "",
            exitCode: session ? 0 : -1,
            shellId: shell_id,
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: `Failed to get shell output: ${error instanceof Error ? error.message : String(error)}`,
            stdout: "",
            stderr: error instanceof Error ? error.message : String(error),
            exitCode: -1,
          };
        }
      },
    };
  };
}

/**
 * Create a shell_kill tool factory
 */
export function createShellKillFactory() {
  return () => {
    const terminalService = getTerminalService();

    return {
      execute: async (params: Record<string, unknown>): Promise<ShellOutputResult> => {
        const { shell_id } = params as { shell_id: string };

        try {
          // Terminate session
          const result = await terminalService.terminateSession(shell_id);

          return {
            success: result.success,
            content: result.stdout || `Shell ${shell_id} terminated successfully`,
            stdout: result.stdout,
            stderr: result.stderr,
            exitCode: result.exitCode,
            shellId: shell_id,
            error: result.error,
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: error instanceof Error ? error.message : String(error),
            stdout: "",
            stderr: error instanceof Error ? error.message : String(error),
            exitCode: -1,
          };
        }
      },
    };
  };
}
