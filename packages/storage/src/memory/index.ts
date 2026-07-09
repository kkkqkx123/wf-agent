/**
 * Memory Storage Adapters
 * Fast, in-memory storage implementations for testing
 */

export { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";
export { MemoryCheckpointStorage } from "./memory-checkpoint-storage.js";
export { MemoryWorkflowStorage } from "./memory-workflow-storage.js";
export { MemoryTaskStorage } from "./memory-task-storage.js";
export { MemoryWorkflowExecutionStorage } from "./memory-workflow-execution-storage.js";
export { MemoryAgentLoopStorage } from "./memory-agent-loop-storage.js";
export { MemoryMetricsStorage } from "./memory-metrics-storage.js";
export { MemoryFileCheckpointStore } from "./memory-file-checkpoint-store.js";

// Export simple memory storage classes (ready-to-use, no custom logic needed)
export {
  MemoryAgentProfileStorage,
  MemoryScriptStorage,
  MemoryToolStorage,
  MemoryHookTemplateStorage,
  MemoryNodeTemplateStorage,
  MemoryTriggerStorage,
} from "./base-memory-storage.js";
