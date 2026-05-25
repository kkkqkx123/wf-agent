/**
 * PTY Executor
 * Pseudo-terminal execution for interactive scripts (stub for Phase 3)
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import { BaseExecutor, type BaseExecuteOptions } from "./base-executor.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PtyExecutor" });

/**
 * PTY Executor
 * Placeholder for pseudo-terminal based execution (fully implemented in Phase 3)
 */
export class PtyExecutor extends BaseExecutor {
  async execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    logger.warn("PTY executor is not yet fully implemented, falling back to direct execution", {
      command: options.command.substring(0, 100),
    });

    return {
      success: false,
      scriptName: "pty",
      executionTime: Date.now() - startTime,
      error: "PTY executor not fully implemented - use direct or shared executor",
    };
  }

  async cleanup(): Promise<void> {
    logger.debug("PTY executor cleanup completed");
  }
}