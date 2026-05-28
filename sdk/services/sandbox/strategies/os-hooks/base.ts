/**
 * OS Hook Strategies — Shared Base
 *
 * Common types and helpers shared across platform-specific OS hook strategies.
 * Each strategy resides in its own file and extends or uses these primitives.
 */

import type { ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { TerminalService } from "../../../terminal/index.js";

// =========================================================================
// Shared fallback: passthrough execution via TerminalService
// =========================================================================

/**
 * Execute a command directly (passthrough) when the native OS hook mechanism
 * is unavailable. All three strategies share this same fallback logic.
 */
export async function executePassthrough(
  terminalService: TerminalService,
  options: StrategyExecuteOptions,
  startTime: number,
): Promise<ScriptExecutionResult> {
  try {
    const result = await terminalService.executeOneOff(options.command, {
      cwd: options.cwd,
      env: options.env,
      timeout: options.timeout,
    });

    return {
      success: result.success,
      scriptName: "sandbox-os-hook",
      stdout: result.stdout,
      stderr: result.stderr,
      exitCode: result.exitCode,
      executionTime: Date.now() - startTime,
      error: result.error,
    };
  } catch (error) {
    return {
      success: false,
      scriptName: "sandbox-os-hook",
      executionTime: Date.now() - startTime,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
