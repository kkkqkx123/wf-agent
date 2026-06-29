/**
 * Persistence Strategy Definition
 *
 * Defines how registries handle data persistence to storage.
 * Two simple strategies for different use cases:
 * - ASYNC: Fire-and-forget, best for performance
 * - BLOCKING: Wait for persistence, better for reliability
 */

/**
 * Persistence Strategy Enum
 *
 * ASYNC: Asynchronous, non-blocking persistence
 * BLOCKING: Synchronous, blocking persistence
 */
export enum PersistenceStrategy {
  /**
   * ASYNC: Asynchronous, non-blocking persistence
   *
   * - Data is persisted in background
   * - Operation returns immediately
   * - Failures are logged but don't affect application flow
   *
   * Use for: High-throughput scenarios where latency matters
   */
  ASYNC = "async",

  /**
   * BLOCKING: Synchronous, blocking persistence
   *
   * - Application waits for persistence to complete
   * - Failures throw exceptions
   * - Memory state matches storage state
   *
   * Use for: Production systems where data integrity is critical
   */
  BLOCKING = "blocking",
}

/**
 * Persistence Configuration
 */
export interface PersistenceConfig {
  strategy?: PersistenceStrategy;
  timeoutMs?: number;
}

/**
 * Default persistence configuration
 */
export const DEFAULT_PERSISTENCE_CONFIG: Required<PersistenceConfig> = {
  strategy: PersistenceStrategy.ASYNC,
  timeoutMs: 30000,
};

/**
 * Merge user config with defaults
 */
export function mergePersistenceConfig(
  userConfig?: PersistenceConfig,
): Required<PersistenceConfig> {
  return {
    ...DEFAULT_PERSISTENCE_CONFIG,
    ...userConfig,
  };
}
