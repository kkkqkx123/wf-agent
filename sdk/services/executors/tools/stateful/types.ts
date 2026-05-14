/**
 * Stateful Executor Type Definition
 */

/**
 * Stateful Executor Configuration
 */
export interface StatefulExecutorConfig {
  /** Whether to enable instance caching */
  enableInstanceCache?: boolean;
  /** Maximum number of cache instances */
  maxCachedInstances?: number;
  /** Instance expiration time (in milliseconds) */
  instanceExpirationTime?: number;
  /** Should expired instances be automatically cleaned up? */
  autoCleanupExpiredInstances?: boolean;
  /** Clean up the interval (in milliseconds) */
  cleanupInterval?: number;
}
