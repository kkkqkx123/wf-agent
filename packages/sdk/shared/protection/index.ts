/**
 * Protection Module
 *
 * Provides execution protection mechanisms including:
 * - ToolFailureProtectionState: Tool failure protection with cooldown
 * - TimeoutManager: Timeout lifecycle management
 */

export { ToolFailureProtectionState } from "./tool-failure-protection-state.js";
export type {
  ToolFailureInfo,
  ToolFailureProtectionConfig,
  ToolFailureProtectionSnapshot,
  ToolExecutionCheckResult,
} from "./tool-failure-protection-types.js";

// Timeout Management
export { TimeoutManager } from "./timeout-manager.js";