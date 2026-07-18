/**
 * IExecutionEntity - Unified interface for all execution entity types
 *
 * Defines the common contract that both AgentLoopEntity and WorkflowExecutionEntity
 * must implement. This provides compile-time guarantees for method consistency
 * across execution instance types, replacing the previous implicit convergence
 * via union types.
 *
 * Design Principles:
 * - Implements Abortable for unified cancellation mechanism
 * - Covers lifecycle control, interruption, hierarchy, and resource cleanup
 * - Uses discriminated union type property for type-safe dispatch
 * - Does NOT include entity-type-specific methods (e.g., steering, variable management)
 *
 * @see AgentLoopEntity - Agent loop execution entity
 * @see WorkflowExecutionEntity - Workflow execution entity
 * @see ExecutionInstance - Union type using this interface for type narrowing
 */

import type {
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
  ID,
} from "@wf-agent/types";
import type { ExecutionInstanceType } from "./execution.js";

/**
 * Interface for components that can be aborted/cancelled
 *
 * Provides a unified cancellation mechanism for execution entities.
 * Separated from StateManager as cancellation is an orthogonal concern:
 * state managers manage data, while abortable entities manage execution flow.
 */
export interface Abortable {
  /**
   * Abort execution with an optional reason
   * @param reason Optional reason for abortion
   */
  abort(reason?: string): void;

  /**
   * Get the AbortSignal associated with this component
   * Can be used to propagate cancellation to child operations
   */
  getAbortSignal(): AbortSignal;

  /**
   * Whether this component has been aborted
   */
  readonly aborted: boolean;
}

/**
 * Unified execution status type
 *
 * Combines statuses from both AgentLoopEntity and WorkflowExecutionEntity.
 * This is a superset of all possible statuses across both domains.
 *
 * Semantic Differences:
 * - Agent uses CANCELLED for both user-cancel and forced-stop scenarios,
 *   while Workflow distinguishes between CANCELLED (user-initiated) and
 *   STOPPED (system-initiated, e.g., timeout or error).
 * - STOPPED is only used in the Workflow domain; Agent domain does not
 *   emit this status.
 * - TIMEOUT is reserved for future use across both domains.
 *
 * When adding cross-domain cascade logic (e.g., a workflow cancelling a
 * child agent loop), the caller must map between these statuses explicitly.
 */
export type ExecutionStatus =
  | "CREATED"
  | "RUNNING"
  | "PAUSED"
  | "COMPLETED"
  | "FAILED"
  | "CANCELLED"
  | "STOPPED"
  | "TIMEOUT";

/**
 * IExecutionEntity - Common interface for all execution entities
 *
 * This interface captures the shared contract between AgentLoopEntity
 * and WorkflowExecutionEntity, providing compile-time enforcement
 * of method consistency.
 */
export interface IExecutionEntity extends Abortable {
  // ============================================================
  // Identity & Type
  // ============================================================

  /** Unique execution instance ID */
  readonly id: string;

  /** Discriminant property for type-safe dispatch */
  readonly instanceType: ExecutionInstanceType;

  // ============================================================
  // Status Access
  // ============================================================

  /** Get current execution status */
  getStatus(): ExecutionStatus;

  /** Check if currently running */
  isRunning(): boolean;

  /** Check if currently paused */
  isPaused(): boolean;

  /** Check if completed */
  isCompleted(): boolean;

  /** Check if failed */
  isFailed(): boolean;

  /** Check if cancelled */
  isCancelled(): boolean;

  // ============================================================
  // Lifecycle Control
  // ============================================================

  /** Pause execution */
  pause(): void;

  /** Resume execution */
  resume(): void;

  /** Stop/cancel execution */
  stop(): void;

  /**
   * Interrupt execution
   * @param type Interrupt type (PAUSE or STOP)
   *
   * When InterruptionState is available:
   * - Delegates to InterruptionState for cascade propagation
   * - Falls back to direct abort if InterruptionState is not set
   */
  interrupt(type: "PAUSE" | "STOP"): void;

  /** Reset interrupt flags and propagate resume to children */
  resetInterrupt(): void;

  /**
   * Check if should pause
   * Checks both internal state and InterruptionState
   */
  shouldPause(): boolean;

  /**
   * Check if should stop
   * Checks both internal state and InterruptionState
   */
  shouldStop(): boolean;

  // ============================================================
  // Hierarchy Management (Unified API)
  // ============================================================

  /** Set parent execution context */
  setParentContext(parentContext: ParentExecutionContext): void;

  /** Get parent execution context */
  getParentContext(): ParentExecutionContext | undefined;

  /** Register child execution reference */
  registerChild(childRef: ChildExecutionReference): void;

  /**
   * Unregister child execution reference
   * @param childId Child execution ID
   * @param childType Child execution type
   */
  unregisterChild(childId: ID, childType: "WORKFLOW" | "AGENT_LOOP"): void;

  /** Get all child execution references */
  getChildReferences(): ChildExecutionReference[];

  /** Get hierarchy depth (0 for root) */
  getHierarchyDepth(): number;

  /** Get root execution ID */
  getRootExecutionId(): ID;

  /** Get root execution type */
  getRootExecutionType(): "WORKFLOW" | "AGENT_LOOP";

  /** Check if this is a root execution (no parent) */
  isRootExecution(): boolean;

  /** Get hierarchy metadata for serialization */
  getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined;

  /**
   * Restore hierarchy from metadata (for checkpoint restoration)
   * @param metadata Hierarchy metadata
   */
  restoreHierarchy(metadata: ExecutionHierarchyMetadata): void;

  // ============================================================
  // Resource Management
  // ============================================================

  /** Cleanup all resources held by this entity */
  cleanup(): void;
}
