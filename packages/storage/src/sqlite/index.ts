/**
 * SQLite storage implementation export
 */

export { BaseSqliteStorage, type BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
export { configurePragmas, type PragmaConfig } from "./sqlite-pragma.js";
export { SqliteCheckpointStorage } from "./sqlite-checkpoint-storage.js";
export { SqliteWorkflowStorage } from "./sqlite-workflow-storage.js";
export { SqliteWorkflowExecutionStorage } from "./sqlite-workflow-execution-storage.js";
export { SqliteTaskStorage } from "./sqlite-task-storage.js";
export { SqliteAgentLoopStorage } from "./sqlite-agent-loop-storage.js";
export { SqliteMetricsStorage, type SqliteMetricsStorageConfig } from "./sqlite-metrics-storage.js";
export { SqliteFileCheckpointStore, type SqliteFileCheckpointStoreConfig } from "./sqlite-file-checkpoint-store.js";
export { SqliteScriptStorage } from "./sqlite-script-storage.js";
export { SqliteToolStorage } from "./sqlite-tool-storage.js";
export { SqliteTriggerStorage } from "./sqlite-trigger-storage.js";
export { SqliteNodeTemplateStorage } from "./sqlite-node-template-storage.js";
export { SqliteHookTemplateStorage } from "./sqlite-hook-template-storage.js";
export { SqliteAgentProfileStorage } from "./sqlite-agent-profile-storage.js";
// Re-export compression utilities for backward compatibility
export {
  compressBlob,
  decompressBlob,
  compressBlobSync,
  decompressBlobSync,
  type CompressionConfig,
  type CompressionResult,
  DEFAULT_COMPRESSION_CONFIG,
} from "@wf-agent/common-utils";

// Note storage
export { SqliteNoteStorage, type SqliteNoteStorageConfig, type NoteEntryResult, type NoteCategorySummary } from "./sqlite-note-storage.js";
