/**
 * Timeout Configuration Types
 * 
 * Configuration interfaces for TimeoutManager and TimeoutRegistry.
 * These are internal to the SDK timeout management system.
 */

/**
 * TimeoutManager configuration
 * 
 * Controls the behavior of individual timeout managers.
 */
export interface TimeoutManagerConfig {
  /** 
   * Default timeout duration in milliseconds
   * Used when duration is not specified in registration
   * @default 30000 (30 seconds)
   */
  defaultTimeout?: number;
  
  /** 
   * Maximum allowed timeout duration in milliseconds
   * Prevents accidentally setting extremely long timeouts
   * @default 86400000 (24 hours)
   */
  maxTimeout?: number;
  
  /** 
   * Enable warning emissions
   * If false, warningThreshold and onWarning are ignored
   * @default true
   */
  enableWarnings?: boolean;
  
  /** 
   * Default warning threshold in milliseconds before timeout
   * Can be overridden per-registration
   * @default 60000 (1 minute)
   */
  defaultWarningThreshold?: number;
  
  /** 
   * Enable metrics collection
   * If true, collects statistics about timeout usage
   * @default true
   */
  enableMetrics?: boolean;

  /**
   * Maximum number of active timeouts per execution
   * Prevents resource exhaustion from too many concurrent timeouts
   * @default 1000
   */
  maxTimeoutsPerExecution?: number;
}

/**
 * TimeoutRegistry configuration
 * 
 * Controls the behavior of the global timeout registry.
 */
export interface TimeoutRegistryConfig {
  /** 
   * Default configuration for TimeoutManager instances
   * Applied to all managers created by this registry
   */
  defaultManagerConfig?: TimeoutManagerConfig;
  
  /** 
   * Auto-cleanup on execution end
   * If true, automatically cancels all timeouts when execution ends
   * @default true
   */
  autoCleanup?: boolean;
  
  /** 
   * Metrics collection interval in milliseconds
   * How often to aggregate and emit metrics
   * @default 60000 (1 minute)
   */
  metricsInterval?: number;
  
  /** 
   * Maximum number of active timeouts per execution
   * Prevents resource exhaustion from too many concurrent timeouts
   * @default 1000
   */
  maxTimeoutsPerExecution?: number;
}

/**
 * Resolved TimeoutManager configuration
 * 
 * All optional fields are filled with defaults.
 */
export interface ResolvedTimeoutManagerConfig {
  defaultTimeout: number;
  maxTimeout: number;
  enableWarnings: boolean;
  defaultWarningThreshold: number;
  enableMetrics: boolean;
  maxTimeoutsPerExecution: number;
}

/**
 * Resolved TimeoutRegistry configuration
 * 
 * All optional fields are filled with defaults.
 */
export interface ResolvedTimeoutRegistryConfig {
  defaultManagerConfig: ResolvedTimeoutManagerConfig;
  autoCleanup: boolean;
  metricsInterval: number;
  maxTimeoutsPerExecution: number;
}

/**
 * Default TimeoutManager configuration values
 */
export const DEFAULT_TIMEOUT_MANAGER_CONFIG: ResolvedTimeoutManagerConfig = {
  defaultTimeout: 30000,           // 30 seconds
  maxTimeout: 86400000,            // 24 hours
  enableWarnings: true,
  defaultWarningThreshold: 60000,  // 1 minute
  enableMetrics: true,
  maxTimeoutsPerExecution: 1000,
};

/**
 * Default TimeoutRegistry configuration values
 */
export const DEFAULT_TIMEOUT_REGISTRY_CONFIG: ResolvedTimeoutRegistryConfig = {
  defaultManagerConfig: DEFAULT_TIMEOUT_MANAGER_CONFIG,
  autoCleanup: true,
  metricsInterval: 60000,          // 1 minute
  maxTimeoutsPerExecution: 1000,
};
