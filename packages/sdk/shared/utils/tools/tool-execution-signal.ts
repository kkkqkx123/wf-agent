/**
 * Tool Execution Signal Utilities
 *
 * Provides utilities for managing abort signals in tool execution,
 * combining timeout and external interruption signals into a single
 * abort signal for consistent error handling.
 */

/**
 * Create a combined abort signal for tool execution
 * Handles both timeout and external interruption signals
 *
 * @param externalSignal Optional external abort signal (e.g., from cancellation)
 * @param timeoutMs Timeout in milliseconds (0 to disable)
 * @returns Combined signal and cleanup function
 *
 * @example
 * ```typescript
 * const { signal, cleanup } = createToolExecutionSignal(
 *   externalSignal,
 *   30000 // 30 second timeout
 * );
 *
 * try {
 *   const result = await someTool({ signal });
 * } finally {
 *   cleanup();
 * }
 * ```
 */
export function createToolExecutionSignal(
  externalSignal: AbortSignal | undefined,
  timeoutMs: number
): {
  signal: AbortSignal;
  cleanup: () => void;
} {
  const controller = new AbortController();
  const cleanupFns: Array<() => void> = [];

  // Setup timeout
  if (timeoutMs > 0) {
    const timeoutId = setTimeout(() => {
      if (!controller.signal.aborted) {
        const timeoutError = new Error(
          `Tool execution timeout after ${timeoutMs}ms`
        ) as Error & {
          source: "timeout";
          timeoutMs: number;
        };
        (timeoutError as any).source = "timeout";
        (timeoutError as any).timeoutMs = timeoutMs;
        controller.abort(timeoutError);
      }
    }, timeoutMs);

    cleanupFns.push(() => clearTimeout(timeoutId));
  }

  // Setup external signal
  if (externalSignal) {
    if (externalSignal.aborted) {
      // Already aborted - create wrapped error with source metadata
      const externalError = new Error(
        String(externalSignal.reason || "External abort")
      ) as Error & {
        source: "external";
        originalReason: unknown;
      };
      (externalError as any).source = "external";
      (externalError as any).originalReason = externalSignal.reason;
      controller.abort(externalError);
    } else {
      // Listen for abort
      const abortHandler = () => {
        if (!controller.signal.aborted) {
          const externalError = new Error(
            String(externalSignal.reason || "External abort")
          ) as Error & {
            source: "external";
            originalReason: unknown;
          };
          (externalError as any).source = "external";
          (externalError as any).originalReason = externalSignal.reason;
          controller.abort(externalError);
        }
      };

      externalSignal.addEventListener("abort", abortHandler, { once: true });

      cleanupFns.push(() => {
        externalSignal.removeEventListener("abort", abortHandler);
      });
    }
  }

  return {
    signal: controller.signal,
    cleanup: () => {
      cleanupFns.forEach(fn => fn());
      if (!controller.signal.aborted) {
        controller.abort();
      }
    },
  };
}

/**
 * Determine if error was caused by timeout
 *
 * @param error The error to check
 * @returns true if the error was caused by a timeout
 */
export function isToolExecutionTimeout(error: Error): boolean {
  return (error as any)?.source === "timeout";
}

/**
 * Determine if error was caused by external abort
 *
 * @param error The error to check
 * @returns true if the error was caused by external abort
 */
export function isToolExecutionExternalAbort(error: Error): boolean {
  return (error as any)?.source === "external";
}

/**
 * Get timeout value from error (if applicable)
 *
 * @param error The error to extract timeout from
 * @returns The timeout value in milliseconds, or null if not a timeout error
 */
export function getToolExecutionTimeoutMs(error: Error): number | null {
  return (error as any)?.timeoutMs ?? null;
}

/**
 * Get original abort reason from external abort error
 *
 * @param error The error to extract reason from
 * @returns The original abort reason, or null if not an external abort error
 */
export function getToolExecutionAbortReason(error: Error): unknown {
  return (error as any)?.originalReason ?? null;
}
