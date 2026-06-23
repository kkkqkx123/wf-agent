/**
 * Tool Failure Protection Types
 *
 * Type definitions for the tool failure protection mechanism.
 */

/**
 * Tool failure tracking information
 */
export interface ToolFailureInfo {
  /** Number of consecutive failures */
  failureCount: number;

  /** Timestamp of last failure */
  lastFailureTimestamp: number;

  /** Last error message */
  lastError?: string;
}

/**
 * Configuration for tool failure protection
 */
export interface ToolFailureProtectionConfig {
  /** Maximum consecutive failures before blocking (default: 3) */
  maxConsecutiveFailures?: number;

  /** Cooldown period in milliseconds after blocking (default: 60000) */
  cooldownPeriod?: number;

  /** Whether the protection is enabled (default: true) */
  enabled?: boolean;
}

/**
 * Snapshot data structure for checkpoint serialization
 */
export interface ToolFailureProtectionSnapshot {
  /** Map of tool name to failure info (serialized as array for JSON compatibility) */
  failureMap: Array<[string, ToolFailureInfo]>;

  /** Configuration parameters */
  config: Required<ToolFailureProtectionConfig>;
}

/**
 * Result when checking if a tool can be executed
 */
export interface ToolExecutionCheckResult {
  /** Whether the tool can be executed */
  allowed: boolean;

  /** Reason if blocked */
  reason?: string;

  /** Current failure count */
  failureCount: number;

  /** Remaining cooldown time in milliseconds (if blocked) */
  remainingCooldown?: number;

  /** Last error message */
  lastError?: string;
}
