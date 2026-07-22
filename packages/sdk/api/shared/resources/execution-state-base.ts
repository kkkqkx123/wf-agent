/**
 * Execution State Base Types
 *
 * Shared type interfaces for execution state management across Agent and Workflow domains.
 * No shared base class — each domain manages its own execution state independently.
 */

// ============================================================================
// Shared Base Types
// ============================================================================

/**
 * Base variable snapshot - shared variable entry
 */
export interface BaseVariableSnapshot {
  name: string;
  value: unknown;
  type: string;
  timestamp: number;
}

/**
 * Base variable state snapshot at a point in time
 */
export interface BaseVariableStateSnapshot {
  executionId: string;
  variables: BaseVariableSnapshot[];
  timestamp: number;
}

/**
 * Base call stack frame
 */
export interface BaseCallStackFrame {
  id: string;
  name?: string;
}

/**
 * Base execution context snapshot
 */
export interface BaseExecutionContextSnapshot {
  executionId: string;
  timestamp: number;
  callStack: BaseCallStackFrame[];
  memoryUsage?: number;
  currentNodeId?: string;
}

/**
 * Base state transition
 */
export interface BaseStateTransition {
  id: string;
  fromState: string;
  toState: string;
  timestamp: number;
  reason?: string;
}

/**
 * Base state transition analysis
 */
export interface BaseStateTransitionAnalysis {
  totalTransitions: number;
  commonTransitions: Array<{ from: string; to: string; count: number; frequency: number }>;
  stateEntryCount: Record<string, number>;
  averageTimeInState: Record<string, number>;
}