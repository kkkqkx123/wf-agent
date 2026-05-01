/**
 * Atomic Serialization-Storage Operations
 *
 * Provides transaction-like semantics for serialize+save operations.
 * Ensures data consistency by treating serialization and storage as a single unit of work.
 */

import { sdkLogger as logger } from "../../utils/logger.js";
import type { Checkpoint } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";
import { WorkflowCheckpointSerializer } from "../serialization/entities/checkpoint-serializer.js";
import { SerializationRegistry } from "../serialization/index.js";

export interface AtomicOperationResult<T = void> {
  success: boolean;
  error?: Error;
  result?: T;
}

/**
 * Performs an atomic save operation that serializes and stores in one step.
 * If either step fails, the entire operation is rolled back (no partial state).
 */
export async function atomicSaveCheckpoint(
  checkpoint: Checkpoint,
  storageAdapter: CheckpointStorageAdapter,
  registry?: SerializationRegistry,
): Promise<AtomicOperationResult> {
  const operationId = crypto.randomUUID();
  
  try {
    logger.debug("Starting atomic checkpoint save", { operationId });

    // Step 1: Serialize
    const serializer = registry?.getSerializer("checkpoint") || new WorkflowCheckpointSerializer();
    
    if (!(serializer instanceof WorkflowCheckpointSerializer)) {
      throw new Error("Invalid checkpoint serializer");
    }

    const serializedData = await serializer.serializeCheckpoint(checkpoint);
    logger.debug("Checkpoint serialized successfully", { 
      operationId,
      size: serializedData.length,
    });

    // Step 2: Save to storage
    const metadata = {
      executionId: checkpoint.executionId,
      workflowId: checkpoint.workflowId,
      timestamp: checkpoint.timestamp,
      version: 1,
    };
    await storageAdapter.save(checkpoint.id, serializedData, metadata);
    logger.debug("Checkpoint saved to storage", { operationId });

    return { success: true };
  } catch (error) {
    logger.error("Atomic checkpoint save failed", { 
      operationId,
      error: error instanceof Error ? error.message : String(error),
    });

    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}

/**
 * Performs an atomic load operation that retrieves and deserializes in one step.
 */
export async function atomicLoadCheckpoint(
  executionId: string,
  storageAdapter: CheckpointStorageAdapter,
  registry?: SerializationRegistry,
): Promise<AtomicOperationResult<Checkpoint | null>> {
  const operationId = crypto.randomUUID();

  try {
    logger.debug("Starting atomic checkpoint load", { operationId, executionId });

    // Step 1: Load from storage
    const serializedData = await storageAdapter.load(executionId);
    
    if (!serializedData) {
      logger.debug("No checkpoint found", { operationId, executionId });
      return { success: true, result: null };
    }

    logger.debug("Checkpoint loaded from storage", { operationId });

    // Step 2: Deserialize
    const serializer = registry?.getSerializer("checkpoint") || new WorkflowCheckpointSerializer();
    
    if (!(serializer instanceof WorkflowCheckpointSerializer)) {
      throw new Error("Invalid checkpoint serializer");
    }

    const checkpoint = await serializer.deserializeCheckpoint(serializedData);
    logger.debug("Checkpoint deserialized successfully", { operationId });

    return { success: true, result: checkpoint };
  } catch (error) {
    logger.error("Atomic checkpoint load failed", {
      operationId,
      executionId,
      error: error instanceof Error ? error.message : String(error),
    });

    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}

/**
 * Batch atomic save for multiple checkpoints.
 * All saves succeed or all fail (transaction semantics).
 */
export async function atomicBatchSaveCheckpoints(
  checkpoints: Array<{ executionId: string; checkpoint: Checkpoint }>,
  storageAdapter: CheckpointStorageAdapter,
  registry?: SerializationRegistry,
): Promise<AtomicOperationResult> {
  const operationId = crypto.randomUUID();
  const savedExecutionIds: string[] = [];

  try {
    logger.debug("Starting batch atomic checkpoint save", {
      operationId,
      count: checkpoints.length,
    });

    const serializer = registry?.getSerializer("checkpoint") || new WorkflowCheckpointSerializer();
    
    if (!(serializer instanceof WorkflowCheckpointSerializer)) {
      throw new Error("Invalid checkpoint serializer");
    }

    // Serialize all first
    const serializedCheckpoints: Array<{ executionId: string; data: Uint8Array }> = [];
    for (const { executionId, checkpoint } of checkpoints) {
      const data = await serializer.serializeCheckpoint(checkpoint);
      serializedCheckpoints.push({ executionId, data });
    }

    logger.debug("All checkpoints serialized", { operationId });

    // Then save all
    for (const { executionId, data } of serializedCheckpoints) {
      const metadata = {
        executionId,
        workflowId: checkpoints.find(c => c.executionId === executionId)?.checkpoint.workflowId || '',
        timestamp: Date.now(),
        version: 1,
      };
      await storageAdapter.save(executionId, data, metadata);
      savedExecutionIds.push(executionId);
    }

    logger.debug("Batch save completed successfully", {
      operationId,
      savedCount: savedExecutionIds.length,
    });

    return { success: true };
  } catch (error) {
    logger.error("Batch atomic save failed, rolling back", {
      operationId,
      error: error instanceof Error ? error.message : String(error),
      savedCount: savedExecutionIds.length,
    });

    // Rollback: delete any checkpoints that were saved before the failure
    for (const executionId of savedExecutionIds) {
      try {
        await storageAdapter.delete(executionId);
        logger.debug("Rolled back checkpoint", { executionId });
      } catch (rollbackError) {
        logger.error("Failed to rollback checkpoint", {
          executionId,
          error: rollbackError,
        });
      }
    }

    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}
