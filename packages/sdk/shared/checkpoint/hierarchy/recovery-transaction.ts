import type {
  RecoveryOperation,
  RecoveryTransactionResult,
  RecoveryTransactionStatus,
  CheckpointEntityType,
} from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "RecoveryTransactionManager" });

export type RollbackStrategy = "all_or_nothing" | "best_effort";

export interface RecoveryTransactionOptions {
  rollbackStrategy: RollbackStrategy;
}

export class RecoveryTransactionManager {
  private operations: Map<string, RecoveryOperation> = new Map();
  private compensatingStack: Array<() => Promise<void>> = [];
  private status: RecoveryTransactionStatus = "pending";
  private readonly transactionId: string;
  private rollbackStrategy: RollbackStrategy;

  constructor(transactionId: string, options?: RecoveryTransactionOptions) {
    this.transactionId = transactionId;
    this.rollbackStrategy = options?.rollbackStrategy ?? "all_or_nothing";
  }

  getTransactionId(): string {
    return this.transactionId;
  }

  getStatus(): RecoveryTransactionStatus {
    return this.status;
  }

  begin(): void {
    if (this.status !== "pending") {
      throw new Error(`Cannot begin transaction in status: ${this.status}`);
    }
    this.status = "in_progress";
    this.operations.clear();
    this.compensatingStack = [];
    logger.info("Recovery transaction started", { transactionId: this.transactionId });
  }

  registerOperation(operation: RecoveryOperation): void {
    this.operations.set(operation.entityId, operation);
  }

  completeOperation(entityId: string): void {
    const operation = this.operations.get(entityId);
    if (operation) {
      operation.status = "completed";
      operation.completedAt = Date.now();
    }
  }

  failOperation(entityId: string, error: string): void {
    const operation = this.operations.get(entityId);
    if (operation) {
      operation.status = "failed";
      operation.error = error;
      operation.completedAt = Date.now();
    }
  }

  addCompensatingAction(action: () => Promise<void>): void {
    this.compensatingStack.push(action);
  }

  async commit(): Promise<RecoveryTransactionResult> {
    const failedOps = Array.from(this.operations.values()).filter(op => op.status === "failed");

    if (failedOps.length > 0) {
      if (this.rollbackStrategy === "all_or_nothing") {
        logger.warn("Transaction has failed operations, initiating rollback", {
          transactionId: this.transactionId,
          failedCount: failedOps.length,
        });
        await this.rollback();
        return this.buildResult("failed");
      }

      logger.warn("Transaction has failed operations, committing with partial success", {
        transactionId: this.transactionId,
        failedCount: failedOps.length,
      });
      this.status = "completed";
      return this.buildResult("completed");
    }

    this.status = "completed";
    logger.info("Recovery transaction committed successfully", {
      transactionId: this.transactionId,
      operationCount: this.operations.size,
    });

    return this.buildResult("completed");
  }

  async rollback(): Promise<void> {
    this.status = "rolled_back";
    logger.info("Rolling back recovery transaction", {
      transactionId: this.transactionId,
      operationsToRollback: this.compensatingStack.length,
    });

    const errors: Array<{ entityId: string; error: string }> = [];

    while (this.compensatingStack.length > 0) {
      const action = this.compensatingStack.pop();
      if (action) {
        try {
          await action();
        } catch (error) {
          errors.push({
            entityId: "compensation",
            error: error instanceof Error ? error.message : String(error),
          });
          logger.error("Compensating action failed during rollback", {
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    if (errors.length > 0) {
      this.status = "rolled_back_with_errors";
      logger.error("Rollback completed with errors", {
        transactionId: this.transactionId,
        errors,
      });
      if (this.rollbackStrategy === "all_or_nothing") {
        throw new Error(`Rollback completed with ${errors.length} compensation failures`);
      }
    }

    logger.info("Rollback completed successfully", {
      transactionId: this.transactionId,
    });
  }

  private buildResult(status: RecoveryTransactionStatus): RecoveryTransactionResult {
    const errors: Array<{ entityId: string; error: string }> = [];
    const recoveredEntities: Array<{ entityId: string; entityType: CheckpointEntityType }> = [];

    for (const op of this.operations.values()) {
      if (op.status === "failed") {
        errors.push({ entityId: op.entityId, error: op.error ?? "Unknown error" });
      } else if (op.status === "completed") {
        recoveredEntities.push({ entityId: op.entityId, entityType: op.entityType });
      }
    }

    return {
      transactionId: this.transactionId,
      status,
      operations: Array.from(this.operations.values()),
      errors,
      recoveredEntities,
    };
  }
}
