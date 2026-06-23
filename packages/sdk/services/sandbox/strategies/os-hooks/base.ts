/**
 * OS Hook Strategies — Shared Base
 *
 * Common types and helpers shared across platform-specific OS hook strategies.
 * Each strategy resides in its own file and extends or uses these primitives.
 */

import type { ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { TerminalService } from "../../../terminal/index.js";

// =========================================================================
// Audit Log
// =========================================================================

export interface AuditEntry {
  timestamp: number;
  strategyId: string;
  command: string;
  allowed: boolean;
  reason: string;
}

const auditLog: AuditEntry[] = [];

/**
 * Record an audit entry for OS hook decision.
 */
export function recordAudit(entry: AuditEntry): void {
  auditLog.push(entry);
  // Keep only last 1000 entries to prevent unbounded memory growth
  if (auditLog.length > 1000) {
    auditLog.splice(0, auditLog.length - 1000);
  }
}

/**
 * Get audit log (read-only snapshot).
 */
export function getAuditLog(): readonly AuditEntry[] {
  return auditLog.slice();
}

/**
 * Clear audit log.
 */
export function clearAuditLog(): void {
  auditLog.length = 0;
}

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
  auditReason = "passthrough (native OS hook unavailable)",
): Promise<ScriptExecutionResult> {
  recordAudit({
    timestamp: startTime,
    strategyId: "os-hook-passthrough",
    command: options.command,
    allowed: true,
    reason: auditReason,
  });

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
