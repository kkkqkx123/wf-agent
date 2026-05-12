/**
 * Control Node Configuration Type Definition
 * Contains ROUTE node configuration
 * 
 * Note: START and END nodes use WorkflowStartConfig and WorkflowEndConfig directly
 * from workflow/boundary-config.ts. No separate StartNodeConfig or EndNodeConfig types needed.
 */

import type { Condition } from "../../graph/condition.js";

/**
 * Routing Node Configuration
 */
export interface RouteNodeConfig {
  /** Routing Rules Array */
  routes: Array<{
    /** conditional expression */
    condition: Condition;
    /** Target Node ID */
    targetNodeId: string;
    /** prioritization */
    priority?: number;
  }>;
  /** Default target node ID */
  defaultTargetNodeId?: string;
}