/**
 * ToolFailureProtectionState - Tool Failure Protection State Manager
 * 
 * Tracks tool failure counts per execution instance and temporarily blocks tools
 * after reaching a threshold to prevent resource waste from repeated failures.
 * 
 * Key Features:
 * - Per-instance isolation (parent/child workflows have independent states)
 * - Consecutive failure tracking
 * - Automatic cooldown and recovery
 * - Checkpoint support via snapshot serialization
 * - Configuration-driven behavior
 */

import { now } from "@wf-agent/common-utils";
import type { StateManager } from "../types/state-manager.js";
import type {
  ToolFailureInfo,
  ToolFailureProtectionConfig,
  ToolFailureProtectionSnapshot,
  ToolExecutionCheckResult,
} from "./tool-failure-protection-types.js";

/**
 * Default configuration values
 */
const DEFAULT_CONFIG: Required<ToolFailureProtectionConfig> = {
  maxConsecutiveFailures: 3,
  cooldownPeriod: 60000, // 60 seconds
  enabled: true,
};

/**
 * ToolFailureProtectionState - Standalone state manager for tool failure protection
 * 
 * Responsibilities:
 * - Track consecutive failure counts per tool
 * - Determine if a tool can be executed based on failure history
 * - Record successful and failed executions
 * - Manage cooldown periods for blocked tools
 * - Provide serialization support for checkpoints
 */
export class ToolFailureProtectionState
  implements StateManager<ToolFailureProtectionSnapshot>
{
  /** Map of tool name to failure tracking info */
  private failureMap: Map<string, ToolFailureInfo> = new Map();
  
  /** Configuration parameters */
  private config: Required<ToolFailureProtectionConfig>;

  constructor(config?: ToolFailureProtectionConfig) {
    this.config = {
      ...DEFAULT_CONFIG,
      ...config,
    };
  }

  // ============================================================================
  // StateManager Interface Implementation
  // ============================================================================

  /**
   * Get the number of tools being tracked
   * @returns Count of tools with failure tracking
   */
  size(): number {
    return this.failureMap.size;
  }

  /**
   * Check if no tools are being tracked
   * @returns true if no failure tracking exists
   */
  isEmpty(): boolean {
    return this.failureMap.size === 0;
  }

  /**
   * Clean up resources
   * Clears all failure tracking data
   */
  cleanup(): void {
    this.failureMap.clear();
  }

  /**
   * Reset all failure tracking to initial state
   */
  reset(): void {
    this.failureMap.clear();
  }

  /**
   * Create a snapshot for checkpoint serialization
   * @returns Snapshot data structure
   */
  createSnapshot(): ToolFailureProtectionSnapshot {
    return {
      failureMap: Array.from(this.failureMap.entries()),
      config: { ...this.config },
    };
  }

  /**
   * Restore from a checkpoint snapshot
   * @param snapshot Previously created snapshot
   */
  restoreFromSnapshot(snapshot: ToolFailureProtectionSnapshot): void {
    this.failureMap = new Map(snapshot.failureMap);
    this.config = { ...snapshot.config };
  }

  // ============================================================================
  // Core Protection Logic
  // ============================================================================

  /**
   * Check if a tool can be executed
   * @param toolName Name of the tool to check
   * @returns Check result with allowed status and metadata
   */
  canExecuteTool(toolName: string): ToolExecutionCheckResult {
    // If protection is disabled, always allow
    if (!this.config.enabled) {
      return {
        allowed: true,
        failureCount: 0,
      };
    }

    const failureInfo = this.failureMap.get(toolName);

    // No failure history, allow execution
    if (!failureInfo) {
      return {
        allowed: true,
        failureCount: 0,
      };
    }

    // Check if tool is currently blocked
    const isBlocked = this.isToolBlocked(failureInfo);

    if (isBlocked) {
      const remainingCooldown = this.getRemainingCooldown(failureInfo);
      return {
        allowed: false,
        reason: `Tool '${toolName}' is blocked due to ${failureInfo.failureCount} consecutive failures`,
        failureCount: failureInfo.failureCount,
        remainingCooldown,
        lastError: failureInfo.lastError,
      };
    }

    // Not blocked, allow execution
    return {
      allowed: true,
      failureCount: failureInfo.failureCount,
      lastError: failureInfo.lastError,
    };
  }

  /**
   * Record a successful tool execution
   * Resets the failure count for the tool
   * @param toolName Name of the tool that succeeded
   */
  recordSuccess(toolName: string): void {
    // Reset failure count on success
    this.failureMap.delete(toolName);
  }

  /**
   * Record a failed tool execution
   * Increments the failure count and updates tracking info
   * @param toolName Name of the tool that failed
   * @param errorMessage Error message from the failure
   */
  recordFailure(toolName: string, errorMessage: string): void {
    const existing = this.failureMap.get(toolName);
    
    if (existing) {
      // Increment existing failure count
      this.failureMap.set(toolName, {
        failureCount: existing.failureCount + 1,
        lastFailureTimestamp: now(),
        lastError: errorMessage,
      });
    } else {
      // First failure for this tool
      this.failureMap.set(toolName, {
        failureCount: 1,
        lastFailureTimestamp: now(),
        lastError: errorMessage,
      });
    }
  }

  /**
   * Manually reset failure tracking for a specific tool
   * @param toolName Name of the tool to reset
   */
  resetTool(toolName: string): void {
    this.failureMap.delete(toolName);
  }

  /**
   * Get current failure count for a tool
   * @param toolName Name of the tool
   * @returns Current consecutive failure count
   */
  getFailureCount(toolName: string): number {
    return this.failureMap.get(toolName)?.failureCount || 0;
  }

  /**
   * Get configuration parameters
   * @returns Current configuration
   */
  getConfig(): Required<ToolFailureProtectionConfig> {
    return { ...this.config };
  }

  /**
   * Update configuration parameters
   * @param config New configuration (partial update supported)
   */
  updateConfig(config: ToolFailureProtectionConfig): void {
    this.config = {
      ...this.config,
      ...config,
    };
  }

  // ============================================================================
  // Internal Helper Methods
  // ============================================================================

  /**
   * Check if a tool is currently blocked based on failure count and cooldown
   * @param failureInfo Failure tracking information
   * @returns true if tool should be blocked
   */
  private isToolBlocked(failureInfo: ToolFailureInfo): boolean {
    // Check if failure count has reached threshold
    if (failureInfo.failureCount < this.config.maxConsecutiveFailures) {
      return false;
    }

    // Check if cooldown period has elapsed
    const elapsed = now() - failureInfo.lastFailureTimestamp;
    return elapsed < this.config.cooldownPeriod;
  }

  /**
   * Calculate remaining cooldown time
   * @param failureInfo Failure tracking information
   * @returns Remaining cooldown time in milliseconds (0 if not blocked)
   */
  private getRemainingCooldown(failureInfo: ToolFailureInfo): number {
    const elapsed = now() - failureInfo.lastFailureTimestamp;
    const remaining = this.config.cooldownPeriod - elapsed;
    return Math.max(0, remaining);
  }
}
