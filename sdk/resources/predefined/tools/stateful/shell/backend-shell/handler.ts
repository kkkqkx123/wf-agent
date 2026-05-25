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
import type { ShellOutputResult } from "./types.js";

/**
 * Create a backend_shell tool factory
 *
 * Delegates all session management and process monitoring to TerminalService.
 */
export function createBackendShellFactory() {
  return () => {
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
