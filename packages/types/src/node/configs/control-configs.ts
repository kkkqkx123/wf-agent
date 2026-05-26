/**
 * Control Node Configuration Type Definition
 * Contains ROUTE node configuration, plus START/END node output types
 * 
 * Note: START and END nodes use WorkflowStartConfig and WorkflowEndConfig directly
 * from workflow/boundary-config.ts. No separate StartNodeConfig or EndNodeConfig types needed.
 */

import type { Condition } from "../../condition.js";

/**
 * Route Node Output
 * - selectedRoute: string - The target node ID of the selected route
 * - evaluatedConditions: Array<{ condition: string, result: boolean, targetNodeId: string }>
 */
export interface RouteNodeOutput {
  selectedRoute: string;
  evaluatedConditions: Array<{
    condition: string;
    result: boolean;
    targetNodeId: string;
  }>;
}

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

/**
 * START / EMBED_START / START_FROM_TRIGGER node output shape.
 * Matches the actual return value of start-handler.ts.
 */
export interface StartNodeOutput {
  message: string;
  input?: unknown;
}

/**
 * END / EMBED_END / CONTINUE_FROM_TRIGGER node output shape.
 * Matches the actual return value of end-handler.ts.
 */
export interface EndNodeOutput {
  output: unknown;
}