/**
 * Control Node Configuration Type Definition
 * Contains START, END, and ROUTE node configurations
 */

import type { Condition } from '../../graph/condition.js';

/**
 * Starting Node Configuration
 * No configuration, only as a workflow start flag
 */
export type StartNodeConfig = object;

/**
 * End Node Configuration
 * No configuration, only as workflow end flag
 */
export type EndNodeConfig = object;

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