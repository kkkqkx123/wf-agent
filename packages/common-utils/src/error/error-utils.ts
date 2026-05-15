/**
 * Error Handling Tool Functions
 * Provides unified error handling capabilities to reduce repetitive type-checking code
 */

// Note: AbortError and InterruptedException have been moved to SDK internal implementation.
// These functions are kept for backward compatibility but use generic Error types.

/**
 * Extracting Error Messages
 * @param error Error object
 * @returns error message string
 */
export function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  if (error === null || error === undefined) {
    return "Unknown error";
  }

  if (typeof error === "object") {
    const err = error as { message?: string; toString?: () => string };
    return err.message || (err.toString ? err.toString() : JSON.stringify(error));
  }

  return String(error);
}

/**
 * Normalize errors to Error objects
 * @param error Error object
 * @returns Error object
 */
export function normalizeError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === "string") {
    return new Error(error);
  }

  if (error === null || error === undefined) {
    return new Error("Unknown error");
  }

  if (typeof error === "object") {
    const err = error as { message?: string; toString?: () => string };
    const message = err.message || (err.toString ? err.toString() : JSON.stringify(error));
    return new Error(message);
  }

  return new Error(String(error));
}

/**
 * Type guard: determine if it is an Error object
 * @param error Error object
 * @returns whether it is an Error object
 */
export function isError(error: unknown): error is Error {
  return error instanceof Error;
}

/**
 * Get error object or undefined
 * @param error Error object
 * @returns Error object or undefined
 */
export function getErrorOrUndefined(error: unknown): Error | undefined {
  return error instanceof Error ? error : undefined;
}

/**
 * Get the error object or create a new Error
 * @param error Error object
 * @returns Error object
 */
export function getErrorOrNew(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}
