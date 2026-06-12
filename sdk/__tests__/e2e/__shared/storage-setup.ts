import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryTaskStorage,
} from "@wf-agent/storage";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  TaskStorageAdapter,
} from "@wf-agent/storage";

export interface StorageBackends {
  checkpoint: CheckpointStorageAdapter;
  workflow: WorkflowStorageAdapter;
  task: TaskStorageAdapter;
}

export function createMemoryStorageBackends(): StorageBackends {
  return {
    checkpoint: new MemoryCheckpointStorage(),
    workflow: new MemoryWorkflowStorage(),
    task: new MemoryTaskStorage(),
  };
}

export async function initializeStorageBackends(backends: StorageBackends): Promise<void> {
  await Promise.all([
    backends.checkpoint.initialize(),
    backends.workflow.initialize(),
    backends.task.initialize(),
  ]);
}

export async function destroyStorageBackends(backends: StorageBackends): Promise<void> {
  await Promise.all([
    backends.checkpoint.clear(),
    backends.workflow.clear(),
    backends.task.clear(),
  ]);
  await Promise.all([
    backends.checkpoint.close(),
    backends.workflow.close(),
    backends.task.close(),
  ]);
}
