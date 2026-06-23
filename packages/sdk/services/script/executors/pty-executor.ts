/**
 * PTY Executor
 * Pseudo-terminal execution for interactive scripts
 * Uses the terminal service to create managed sessions for command execution
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import { BaseExecutor, type BaseExecuteOptions } from "./base-executor.js";
import type { TerminalService, TerminalSessionOptions } from "../../../services/terminal/index.js";
import { getTerminalService } from "../../../services/terminal/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PtyExecutor" });

/**
 * PTY Executor
 * Executes commands in a managed pseudo-terminal session
 * Supports interactive input/output via the terminal service
 */
export class PtyExecutor extends BaseExecutor {
  private terminalService: TerminalService;
  /** Map of command to active session ID for input routing */
  private activeSessions: Map<string, string> = new Map();

  constructor(terminalService?: TerminalService) {
    super();
    this.terminalService = terminalService ?? getTerminalService();
  }

  /**
   * Execute a command in a PTY session
   * Creates a new session, executes the command, and stores the session for potential interactive input
   */
  async execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const commandKey = options.command.substring(0, 100);

    try {
      const sessionOptions: TerminalSessionOptions = {};

      if (options.cwd) {
        sessionOptions.cwd = options.cwd;
      }
      if (options.env && Object.keys(options.env).length > 0) {
        sessionOptions.env = options.env;
      }

      const session = await this.terminalService.createSession(sessionOptions);

      this.activeSessions.set(commandKey, session.sessionId);

      const result = await this.terminalService.executeInSession(
        session.sessionId,
        options.command,
        { timeout: options.timeout },
      );

      return {
        success: result.success,
        scriptName: "pty",
        stdout: result.stdout,
        stderr: result.stderr,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      logger.error("PTY execution failed", {
        command: options.command.substring(0, 100),
        error: error instanceof Error ? error.message : String(error),
      });

      return {
        success: false,
        scriptName: "pty",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Get the active session ID for a command
   * Used by the coordinator to send interactive input
   */
  getSessionId(command: string): string | undefined {
    const commandKey = command.substring(0, 100);
    return this.activeSessions.get(commandKey);
  }

  /**
   * Cleanup all active sessions
   */
  async cleanup(): Promise<void> {
    for (const [key, sessionId] of this.activeSessions) {
      try {
        await this.terminalService.terminateSession(sessionId);
        logger.debug("PTY session terminated", { sessionId, command: key });
      } catch (error) {
        logger.warn("Failed to terminate PTY session", {
          sessionId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
    this.activeSessions.clear();
    logger.debug("PTY executor cleanup completed");
  }
}
