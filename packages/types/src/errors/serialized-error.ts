/**
 * Serialized Error Representation
 *
 * Error objects cannot be directly serialized with JSON.stringify,
 * so we convert them to this format for persistence or transmission.
 */

/**
 * Serialized error representation
 */
export interface SerializedError {
  /** Error message */
  message: string;
  /** Error name (constructor name) */
  name: string;
  /** Stack trace */
  stack?: string;
  /** Cause of the error */
  cause?: unknown;
}
