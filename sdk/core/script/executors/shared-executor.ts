/**
 * Shared Executor
 * Uses persistent shell sessions for stateful execution
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import { getTerminalService, type TerminalService } from "../../../services/terminal/index.js";
import { BaseExecutor, type BaseExecuteOptions } from "./base-executor.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "SharedExecutor" });

/**
 * Shared Executor
 * Creates and reuses shell sessions for stateful command execution
 */
export class SharedExecutor extends BaseExecutor {
  private terminalService: TerminalService;
  private sessionId: string | null = null;
  private sessionCwd: string | null = null;

  constructor(terminalService?: TerminalService) {
    super();
    this.terminalService = terminalService || getTerminalService();
  }

  async execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const { command, cwd, env, timeout } = options;

    try {
      if (!this.sessionId || (cwd && cwd !== this.sessionCwd)) {
        if (this.sessionId) {
          await this.terminalService.terminateSession(this.sessionId);
        }
        const session = await this.terminalService.createSession({
          cwd,
          env,
        });
        this.sessionId = session.sessionId;
        this.sessionCwd = cwd || null;
      }

      const result = await this.terminalService.executeInSession(
        this.sessionId,
        command,
        { timeout },
      );

      return {
        success: result.success,
        scriptName: "shared",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "shared",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async cleanup(): Promise<void> {
    if (this.sessionId) {
      await this.terminalService.terminateSession(this.sessionId);
      this.sessionId = null;
      this.sessionCwd = null;
      logger.debug("Shared executor session terminated");
    }
  }
}