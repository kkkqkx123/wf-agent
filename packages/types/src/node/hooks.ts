/**
 * Node Hook Related Type Definitions
 */

import type { BaseHookConfig } from "../hook.js";

/**
 * Hook Type
 */
export type HookType =
  /** Triggered before node execution */
  | "BEFORE_EXECUTE"
  /** Triggered after node execution */
  | "AFTER_EXECUTE";

/**
 * Node Hook Configuration
 */
export type NodeHook = BaseHookConfig<HookType>;
