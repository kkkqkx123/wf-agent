/**
 * JSON storage implementation export
 *
 * JSON backend is designed for persistent demos and prototyping.
 * It is NOT suitable for production use due to:
 * - No atomic write guarantees (file corruption risk under concurrent access)
 * - No built-in optimize/vacuum (accumulates orphaned files over time)
 * - No WAL or transaction isolation
 * - High I/O overhead for large datasets
 *
 * For production use, prefer SQLite or PostgreSQL backends.
 */

export { BaseJsonStorage, type BaseJsonStorageConfig } from "./base-json-storage.js";
export { JsonCheckpointStorage } from "./json-checkpoint-storage.js";
export { JsonWorkflowExecutionStorage } from "./json-workflow-execution-storage.js";
export { JsonWorkflowStorage } from "./json-workflow-storage.js";
export { JsonTaskStorage } from "./json-task-storage.js";
// NoteStorage: Lightweight session notes storage for development/debugging scenarios.
// Only available in JSON backend as it's primarily used for local file-based workflows.
// Other backends (Memory, Postgres, SQLite) can add NoteStorage if needed.
export { JsonNoteStorage, type NoteEntry, type NoteMetadata } from "./json-note-storage.js";
export { JsonAgentLoopStorage } from "./json-agent-loop-storage.js";
export { JsonMetricsStorage, type JsonMetricsStorageConfig } from "./json-metrics-storage.js";
export { JsonFileCheckpointStore, type JsonFileCheckpointStoreConfig } from "./json-file-checkpoint-store.js";

// Template Storage Implementations
export { JsonTriggerStorage } from "./json-trigger-storage.js";
export { JsonToolStorage } from "./json-tool-storage.js";
export { JsonScriptStorage } from "./json-script-storage.js";
export { JsonNodeTemplateStorage } from "./json-node-template-storage.js";
export { JsonHookTemplateStorage } from "./json-hook-template-storage.js";
export { JsonAgentProfileStorage } from "./json-agent-profile-storage.js";
