import type { ExecutionType, ID } from "@wf-agent/types";
import type { IExecutionEntity } from "../../types/execution-entity.js";
import type { ChildExecutionReference } from "@wf-agent/types";

export interface ChildRestoreDependencies {
  findCheckpoint: (childId: ID, childType: ExecutionType) => Promise<ID | undefined>;
  findCheckpointsBatch?: (childRefs: ChildExecutionReference[]) => Promise<Map<string, string | undefined>>;
  restoreEntity: (checkpointId: ID, childType: ExecutionType, parentId: ID) => Promise<IExecutionEntity>;
  registerChild: (parent: IExecutionEntity, child: IExecutionEntity, childRef: ChildExecutionReference) => void;
  onChildRestored?: (child: IExecutionEntity) => Promise<void>;
}

export interface RestoreStrategy {
  readonly executionType: ExecutionType;

  findCheckpoint(childId: ID): Promise<ID | undefined>;

  restoreEntity(checkpointId: ID, _parentId?: ID): Promise<IExecutionEntity>;

  registerChild(parent: IExecutionEntity, child: IExecutionEntity, childRef: ChildExecutionReference): void;

  onChildRestored?: (child: IExecutionEntity) => Promise<void>;
}

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
