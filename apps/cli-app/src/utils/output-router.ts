/**
 * Output Router
 *
 * Centralized routing layer between command handlers and output streams.
 * Handles three output modes:
 *   - text:  Delegates to entity formatters for human-readable output
 *   - json:  Wraps data in a structured envelope and outputs as JSON
 *   - silent: Suppresses all output
 *
 * This is the single entry point for all command output, ensuring consistent
 * behavior across output modes and eliminating the ad-hoc pattern where
 * each command calls CLIOutput.output() directly.
 */

import { getOutput, type CLIOutput } from "./output.js";
import { isJsonMode, isSilentMode } from "./mode-detector.js";
import { CLIError } from "../types/cli-types.js";

// ============================================
// Types
// ============================================

export type RenderType = "list" | "detail" | "action";

export interface RenderDescriptor {
  /** Rendering type: list (collection), detail (single item), action (operation result) */
  type: RenderType;
  /** Entity name for JSON envelope (e.g. "workflow", "message") */
  entity?: string;
  /**
   * Lazy text formatter — only called in text mode.
   * In json or silent mode this callback is never invoked.
   */
  format?: () => string;
  /** Success message for action type (text mode) */
  message?: string;
  /** Additional metadata included in JSON mode */
  metadata?: Record<string, unknown>;
}

export interface ErrorDescriptor {
  message: string;
  code?: string;
}

// ============================================
// OutputRouter
// ============================================

export class OutputRouter {
  private output: CLIOutput;

  constructor(output?: CLIOutput) {
    this.output = output ?? getOutput();
  }

  /**
   * Render structured data to output, automatically selecting
   * the appropriate mode (json / text / silent).
   *
   * - json mode:  Outputs `{ success, type, entity, data, message, metadata, timestamp }`
   * - silent mode: No output (format callback is never called)
   * - text mode:  Calls `descriptor.format()` or falls back to descriptor.message or String(data)
   *
   * @throws {CLIError} If the format callback throws, wraps the error with render context.
   */
  render<T>(data: T, descriptor: RenderDescriptor): void {
    if (isSilentMode()) return;

    if (isJsonMode()) {
      this.output.structuredOutput({
        success: true,
        type: descriptor.type,
        entity: descriptor.entity,
        data,
        message: descriptor.message,
        metadata: descriptor.metadata,
        timestamp: new Date().toISOString(),
      });
      return;
    }

    // Text mode
    if (descriptor.type === "action") {
      this.output.success(descriptor.message ?? "Operation completed");
      return;
    }

    if (descriptor.format) {
      try {
        this.output.output(descriptor.format());
      } catch (cause) {
        const ctx = `format callback failed for ${descriptor.type}${descriptor.entity ? ` (${descriptor.entity})` : ""}`;
        const message = cause instanceof Error ? `${ctx}: ${cause.message}` : ctx;
        throw new CLIError(message, "UNKNOWN_ERROR", 1);
      }
    } else {
      // Fallback: produce readable output even without a format callback
      let output: string;
      if (data === null || data === undefined) {
        output = "";
      } else if (typeof data === "object") {
        try {
          output = JSON.stringify(data, null, 2);
        } catch {
          output = String(data);
        }
      } else {
        output = String(data);
      }
      this.output.output(output);
    }
  }

  /**
   * Render an error to output, automatically selecting the appropriate mode.
   */
  error(err: ErrorDescriptor): void {
    if (isSilentMode()) return;

    if (isJsonMode()) {
      this.output.structuredOutput({
        success: false,
        error: { message: err.message, code: err.code },
        timestamp: new Date().toISOString(),
      });
    } else {
      this.output.fail(err.message);
    }
  }
}

// ============================================
// Global Instance Management
// ============================================

let globalRouter: OutputRouter | null = null;

/**
 * Initialize global output router.
 */
export function initializeRouter(): OutputRouter {
  globalRouter = new OutputRouter();
  return globalRouter;
}

/**
 * Get global output router.
 */
export function getRouter(): OutputRouter {
  if (!globalRouter) {
    return initializeRouter();
  }
  return globalRouter;
}

/**
 * Reset global output router (for testing).
 */
export function resetRouter(): void {
  globalRouter = null;
}
