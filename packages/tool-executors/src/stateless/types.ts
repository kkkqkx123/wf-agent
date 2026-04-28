/**
 * Stateless Executor Type Definition
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Function registry entries
 */
export interface FunctionRegistryItem {
  /** function */
  execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>;
  /** Version */
  version?: string;
  /** Description */
  description?: string;
  /** Registration time */
  registeredAt: Date;
  /** Call count */
  callCount: number;
  /** Last call time */
  lastCalledAt?: Date;
}

/**
 * Function registry configuration
 */
export interface FunctionRegistryConfig {
  /** Whether to enable version control */
  enableVersionControl: boolean;
  /** Whether to record call statistics */
  enableCallStatistics: boolean;
  /** Maximum number of registered functions */
  maxFunctions: number;
}
