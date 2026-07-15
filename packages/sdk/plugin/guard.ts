/**
 * Plugin Guard - Provides stability guarantees for plugin execution.
 *
 * NOT a security sandbox — plugins are considered trusted.
 * Responsibilities:
 * - Timeout enforcement (prevents infinite loops from blocking the engine)
 * - Error isolation (plugin errors don't propagate to the engine)
 */

export interface PluginGuardOptions {
  /** Max execution time per plugin lifecycle hook (ms). 0 = no limit, default 10000. */
  timeout: number;
}

/**
 * Plugin Guard - Executes plugin functions with stability guarantees.
 *
 * This is NOT a security sandbox. It provides:
 * 1. Timeout enforcement — prevents plugin bugs from blocking the engine
 * 2. Error isolation — plugin errors are wrapped, not propagated raw
 */
export class PluginGuard {
  constructor(private options: PluginGuardOptions) {}

  /**
   * Execute a plugin function with timeout and error isolation.
   * If the function times out, a PluginGuardError is thrown.
   * If the function throws, the error is wrapped in a PluginGuardError
   * to prevent plugin errors from propagating directly.
   */
  async execute<T>(pluginId: string, fn: () => Promise<T>): Promise<T> {
    try {
      if (this.options.timeout > 0) {
        return await this.executeWithTimeout(pluginId, fn);
      }
      return await fn();
    } catch (error) {
      // Wrap the error to isolate plugin failures
      throw new PluginGuardError(
        `Plugin '${pluginId}' execution failed: ${error instanceof Error ? error.message : String(error)}`,
        pluginId,
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Execute a function with a timeout.
   */
  private async executeWithTimeout<T>(pluginId: string, fn: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new PluginGuardError(
          `Plugin '${pluginId}' timed out after ${this.options.timeout}ms`,
          pluginId,
        ));
      }, this.options.timeout);

      fn()
        .then(result => {
          clearTimeout(timer);
          resolve(result);
        })
        .catch(error => {
          clearTimeout(timer);
          reject(error);
        });
    });
  }
}

/**
 * Error thrown when a plugin guard violation occurs.
 */
export class PluginGuardError extends Error {
  constructor(
    message: string,
    public readonly pluginId: string,
    public override readonly cause?: Error,
  ) {
    super(message);
    this.name = 'PluginGuardError';
  }
}