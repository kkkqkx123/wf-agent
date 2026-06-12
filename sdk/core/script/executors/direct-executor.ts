/**
 * Direct Executor
 * Stateless one-off execution via TerminalService.executeOneOff()
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import { getTerminalService, type TerminalService } from "../../../services/terminal/index.js";
import { BaseExecutor, type BaseExecuteOptions } from "./base-executor.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "DirectExecutor" });

/**
 * Direct Executor
 * Executes commands as one-off (stateless) operations
 */
export class DirectExecutor extends BaseExecutor {
  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    super();
    this.terminalService = terminalService || getTerminalService();
  }

  async execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const { command, cwd, env, timeout } = options;

    try {
      const result = await this.terminalService.executeOneOff(command, {
        cwd,
        env,
        timeout,
      });

      return {
        success: result.success,
        scriptName: "direct",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "direct",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async cleanup(): Promise<void> {
    logger.debug("Direct executor cleanup completed");
  }
}
