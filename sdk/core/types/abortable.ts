/**
 * Abortable - Execution cancellation interface
 *
 * Provides a unified cancellation mechanism for execution entities.
 * Separated from StateManager as cancellation is an orthogonal concern:
 * state managers manage data, while abortable entities manage execution flow.
 */

/**
 * Interface for components that can be aborted/cancelled
 */
export interface Abortable {
  /**
   * Abort execution with an optional reason
   * @param reason Optional reason for abortion
   */
  abort(reason?: string): void;

  /**
   * Get the AbortSignal associated with this component
   * Can be used to propagate cancellation to child operations
   */
  getAbortSignal(): AbortSignal;

  /**
   * Whether this component has been aborted
   */
  readonly aborted: boolean;
}
