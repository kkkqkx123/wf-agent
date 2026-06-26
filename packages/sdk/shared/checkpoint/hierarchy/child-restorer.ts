import type {
  ChildExecutionReference,
  ExecutionType,
  ID,
} from "@wf-agent/types";
import type { IExecutionEntity } from "../../types/execution-entity.js";
import type { ChildCheckpointResolver } from "./child-resolver.js";
import { RecoveryTransactionManager } from "./recovery-transaction.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { RestoreStrategyRegistry } from "./restore-strategy.js";

const logger = createContextualLogger({ component: "ChildCheckpointRestorer" });

export interface ChildRestoreResult {
  childId: ID;
  childType: ExecutionType;
  success: boolean;
  entity?: IExecutionEntity;
  error?: string;
}

export interface ChildRestoreDependencies {
  findCheckpoint: (childId: ID, childType: ExecutionType) => Promise<ID | undefined>;

  findCheckpointsBatch?: (childRefs: ChildExecutionReference[]) => Promise<Map<string, string | undefined>>;

  restoreEntity: (
    checkpointId: ID,
    childType: ExecutionType,
    parentId: ID,
  ) => Promise<IExecutionEntity>;

  registerChild: (
    parent: IExecutionEntity,
    child: IExecutionEntity,
    childRef: ChildExecutionReference,
  ) => void;

  onChildRestored?: (child: IExecutionEntity) => Promise<void>;

  checkpointResolver?: ChildCheckpointResolver;

  transactionManager?: RecoveryTransactionManager;

  maxConcurrency?: number;

  maxDepth?: number;

  strategyRegistry?: RestoreStrategyRegistry;
}

export class ChildCheckpointRestorer {
  private static readonly DEFAULT_MAX_CONCURRENCY = 5;
  private static readonly DEFAULT_MAX_DEPTH = 100;

   async restoreChildren(
     parentEntity: IExecutionEntity,
     childRefs: ChildExecutionReference[],
     deps: ChildRestoreDependencies,
     resolver?: ChildCheckpointResolver,
   ): Promise<ChildRestoreResult[]> {
     if (childRefs.length === 0) {
       return [];
     }

     const maxConcurrency = deps.maxConcurrency ?? ChildCheckpointRestorer.DEFAULT_MAX_CONCURRENCY;
     const maxDepth = deps.maxDepth ?? ChildCheckpointRestorer.DEFAULT_MAX_DEPTH;

     const effectiveDeps = resolver
       ? { ...deps, checkpointResolver: resolver }
       : deps;

      const allResults: ChildRestoreResult[] = [];
     const visited = new Set<ID>([parentEntity.id]);
     const restorationPath: ID[] = [];
     let maxDepthReached = 0;
     let totalChildrenRestored = 0;

      if (effectiveDeps.findCheckpointsBatch) {
       const checkpointMap = await effectiveDeps.findCheckpointsBatch(childRefs);
       for (const [childId, checkpointId] of checkpointMap) {
         const childRef = childRefs.find(r => r.childId === childId);
         if (childRef && !checkpointId) {
           allResults.push({
             childId,
             childType: childRef.childType,
             success: false,
             error: "No checkpoint found",
           });
         }
       }
     }

     const rootStack: Array<{
       parentEntity: IExecutionEntity;
       childRefs: ChildExecutionReference[];
       depth: number;
     }> = [{
       parentEntity,
       childRefs,
       depth: 0,
     }];

     const semaphore = new Semaphore(maxConcurrency);

     while (rootStack.length > 0) {
       const current = rootStack.pop()!;

       if (current.depth >= maxDepth) {
         for (const childRef of current.childRefs) {
           allResults.push({
             childId: childRef.childId,
             childType: childRef.childType,
             success: false,
             error: `Max depth exceeded (${maxDepth})`,
           });
         }
         continue;
       }

       const workflowChildren = current.childRefs.filter(c => c.childType === "WORKFLOW");
       const agentChildren = current.childRefs.filter(c => c.childType === "AGENT_LOOP");
       const orderedChildren = [...workflowChildren, ...agentChildren];

       const restoreTasks = orderedChildren.map(async (childRef) => {
         if (visited.has(childRef.childId)) {
           const cyclePath = [...restorationPath, childRef.childId].join(" -> ");
           logger.warn("Cycle detected in hierarchy", {
             childId: childRef.childId,
             childType: childRef.childType,
             cyclePath,
           });
           return {
             childId: childRef.childId,
             childType: childRef.childType,
             success: false,
             error: `Cycle detected: already visited at path [${cyclePath}]`,
           } as ChildRestoreResult;
         }

         await semaphore.acquire();

         try {
           restorationPath.push(childRef.childId);
           return await this.restoreSingleChildWithStack(
             current.parentEntity,
             childRef,
             effectiveDeps,
             visited,
             rootStack,
             current.depth + 1,
             maxDepth,
           );
         } finally {
           restorationPath.pop();
           semaphore.release();
         }
       });

       const results = await Promise.all(restoreTasks);
       allResults.push(...results);

       const succeeded = results.filter(r => r.success).length;
       totalChildrenRestored += succeeded;
       maxDepthReached = Math.max(maxDepthReached, current.depth + 1);

       if (current.depth > 10) {
         logger.warn("Deep hierarchy detected during restoration", {
           currentDepth: current.depth,
           maxDepthReached,
           totalChildrenRestored,
           parentId: current.parentEntity.id,
         });
       }
      }

     logger.info("Child restoration completed", {
       parentId: parentEntity.id,
       totalChildrenRestored,
       maxDepthReached,
       totalResults: allResults.length,
       succeeded: allResults.filter(r => r.success).length,
       failed: allResults.filter(r => !r.success).length,
     });

     return allResults;
   }

   private async restoreSingleChildWithStack(
     parentEntity: IExecutionEntity,
     childRef: ChildExecutionReference,
     deps: ChildRestoreDependencies,
     visited: Set<ID>,
     rootStack: Array<{ parentEntity: IExecutionEntity; childRefs: ChildExecutionReference[]; depth: number }>,
     depth: number,
     maxDepth: number,
   ): Promise<ChildRestoreResult> {
     try {
       const strategy = deps.strategyRegistry?.get(childRef.childType);

       let checkpointId: ID | undefined;
       let childEntity: IExecutionEntity;

        if (strategy) {
          checkpointId = await strategy.findCheckpoint(childRef.childId);
          if (!checkpointId) {
            return {
              childId: childRef.childId,
              childType: childRef.childType,
              success: false,
              error: "No checkpoint found",
            };
          }
          childEntity = await strategy.restoreEntity(checkpointId);
        } else {
         if (deps.checkpointResolver) {
           const descriptor = await deps.checkpointResolver.resolveLatestCheckpoint(childRef);
           checkpointId = descriptor?.checkpointId;
         } else {
           checkpointId = await deps.findCheckpoint(childRef.childId, childRef.childType);
         }

         if (!checkpointId) {
           return {
             childId: childRef.childId,
             childType: childRef.childType,
             success: false,
             error: "No checkpoint found",
           };
         }
         childEntity = await deps.restoreEntity(
           checkpointId,
           childRef.childType,
           parentEntity.id,
         );
       }

       childEntity.setParentContext({
         parentType: parentEntity.instanceType === "workflowExecution" ? "WORKFLOW" : "AGENT_LOOP",
         parentId: parentEntity.id,
       });

       if (strategy) {
         strategy.registerChild(parentEntity, childEntity, childRef);
       } else {
         deps.registerChild(parentEntity, childEntity, childRef);
       }

       deps.transactionManager?.registerOperation({
         operationId: `restore_${childRef.childId}`,
         entityId: childRef.childId,
         entityType: childRef.childType === "WORKFLOW" ? "workflow" : "agent",
         checkpointId,
         status: "in_progress",
         startedAt: Date.now(),
         compensatingActions: [
           async () => {
             logger.info("Rolling back child registration", { childId: childRef.childId });
           },
         ],
       });

       if (strategy?.onChildRestored) {
         await strategy.onChildRestored(childEntity);
       } else if (deps.onChildRestored) {
         await deps.onChildRestored(childEntity);
       }

       deps.transactionManager?.completeOperation(childRef.childId);

       visited.add(childRef.childId);

       const grandChildRefs = childEntity.getChildReferences();
       if (grandChildRefs.length > 0 && depth < maxDepth) {
         rootStack.push({
           parentEntity: childEntity,
           childRefs: grandChildRefs,
           depth,
         });
       } else if (grandChildRefs.length > 0) {
         logger.warn("Max depth reached, skipping grandchildren", {
           childId: childRef.childId,
           grandChildCount: grandChildRefs.length,
           maxDepth,
         });
       }

       return {
         childId: childRef.childId,
         childType: childRef.childType,
         success: true,
         entity: childEntity,
       };
     } catch (error) {
       visited.add(childRef.childId);

       deps.transactionManager?.failOperation(
         childRef.childId,
         error instanceof Error ? error.message : String(error),
       );
       return {
         childId: childRef.childId,
         childType: childRef.childType,
         success: false,
         error: error instanceof Error ? error.message : String(error),
       };
     }
   }

  static summarizeResults(results: ChildRestoreResult[]): {
    total: number;
    succeeded: number;
    failed: number;
    failures: Array<{ childId: ID; childType: ExecutionType; error: string }>;
  } {
    const failures = results
      .filter(r => !r.success)
      .map(r => ({
        childId: r.childId,
        childType: r.childType,
        error: r.error ?? "Unknown error",
      }));

    return {
      total: results.length,
      succeeded: results.filter(r => r.success).length,
      failed: failures.length,
      failures,
    };
  }
}

class Semaphore {
  private permits: number;
  private queue: Array<() => void> = [];

  constructor(permits: number) {
    this.permits = permits;
  }

  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return;
    }

    return new Promise<void>(resolve => {
      this.queue.push(resolve);
    });
  }

  release(): void {
    if (this.queue.length > 0) {
      const next = this.queue.shift()!;
      next();
    } else {
      this.permits++;
    }
  }
}
