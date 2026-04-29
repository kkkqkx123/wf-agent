/**
 * Execution event type definitions
 * For asynchronous execution and observers used with ExecutionBuilder
 */

import type { WorkflowExecutionResult } from "@wf-agent/types";

/**
 * Execute event type
 */
export type ExecutionEvent =
  | StartEvent
  | CompleteEvent
  | ErrorEvent
  | CancelledEvent
  | ProgressEvent
  | NodeExecutedEvent;

/**
 * Start event
 */
export interface StartEvent {
  type: "start";
  timestamp: number;
  workflowId: string;
}

/**
 * Complete the event
 */
export interface CompleteEvent {
  type: "complete";
  timestamp: number;
  workflowId: string;
  executionId: string;
  result: WorkflowExecutionResult;
  executionStats: {
    duration: number;
    steps: number;
    nodesExecuted: number;
  };
}

/**
 * Error event
 */
export interface ErrorEvent {
  type: "error";
  timestamp: number;
  workflowId: string;
  executionId: string;
  error: Error;
}

/**
 * Cancel event
 */
export interface CancelledEvent {
  type: "cancelled";
  timestamp: number;
  workflowId: string;
  executionId: string;
  reason: string;
}

/**
 * Progress events
 */
export interface ProgressEvent {
  type: "progress";
  timestamp: number;
  workflowId: string;
  executionId: string;
  progress: {
    status: "running" | "paused" | "completed" | "failed" | "cancelled";
    currentStep: number;
    totalSteps?: number;
    currentNodeId: string;
    currentNodeType: string;
  };
}

/**
 * Node Execution Events
 */
export interface NodeExecutedEvent {
  type: "nodeExecuted";
  timestamp: number;
  workflowId: string;
  executionId: string;
  nodeId: string;
  nodeType: string;
  nodeResult: unknown;
  executionTime: number;
}
