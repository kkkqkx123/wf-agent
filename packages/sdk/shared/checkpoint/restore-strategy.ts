/**
 * Restore Strategy Interface
 *
 * Encapsulates entity-type-specific restoration logic.
 * Implementations handle restoration for a specific execution type (WORKFLOW, AGENT_LOOP).
 */

import type { ExecutionType, ID } from "@wf-agent/types";
import type { IExecutionEntity } from "../../shared/types/execution-entity.js";
import type { ChildExecutionReference } from "@wf-agent/types";

/**
 * Dependencies for restoring a single child execution
 */
export interface ChildRestoreDependencies {
  findCheckpoint: (childId: ID, childType: ExecutionType) => Promise<ID | undefined>;
  findCheckpointsBatch?: (childRefs: ChildExecutionReference[]) => Promise<Map<string, string | undefined>>;
  restoreEntity: (checkpointId: ID, childType: ExecutionType, parentId: ID) => Promise<IExecutionEntity>;
  registerChild: (parent: IExecutionEntity, child: IExecutionEntity, childRef: ChildExecutionReference) => void;
  onChildRestored?: (child: IExecutionEntity) => Promise<void>;
}

/**
 * Strategy for restoring child executions of a specific type.
 * Each execution type (WORKFLOW, AGENT_LOOP) provides its own restoration logic.
 */
export interface RestoreStrategy {
  /**
   * The execution type this strategy handles
   */
  readonly executionType: ExecutionType;

  /**
   * Find the latest checkpoint ID for a child execution
   */
  findCheckpoint(childId: ID): Promise<ID | undefined>;

  /**
   * Restore a child execution entity from checkpoint
   * @param checkpointId The checkpoint ID to restore from
   * @param _parentId The parent entity ID (unused in most implementations)
   */
  restoreEntity(checkpointId: ID, _parentId?: ID): Promise<IExecutionEntity>;

  /**
   * Register a restored child with its parent
   */
  registerChild(parent: IExecutionEntity, child: IExecutionEntity, childRef: ChildExecutionReference): void;

  /**
   * Optional post-registration hook
   */
  onChildRestored?: (child: IExecutionEntity) => Promise<void>;
}

/**
 * Registry for restore strategies.
 * Maps execution types to their corresponding restoration strategies.
 */
export class RestoreStrategyRegistry {
  private strategies = new Map<ExecutionType, RestoreStrategy>();

  register(strategy: RestoreStrategy): void {
    this.strategies.set(strategy.executionType, strategy);
  }

  get(executionType: ExecutionType): RestoreStrategy | undefined {
    return this.strategies.get(executionType);
  }

  has(executionType: ExecutionType): boolean {
    return this.strategies.has(executionType);
  }

  getAll(): RestoreStrategy[] {
    return Array.from(this.strategies.values());
  }
}
