/**
 * @wf-agent/storage
 * The storage package, which provides a variety of storage backend implementations
 *
 * Design Principles:
 * - SDK only relies on the StorageCallback interface
 * - packages provide specific implementations for use by the apps layer
 * - The apps layer can choose to use the built-in implementation or implement their own interfaces.
 *
 * Supported storage types:
 * - Checkpoint
 * - Workflow-Execution
 * - Workflow
 * - Task
 *
 * Supported storage backends:
 * - SQLite database storage
 * - PostgreSQL database storage
 */

// Logger Export
export { logger, createModuleLogger } from "./logger.js";

// Core Type Definition
export * from "./types/index.js";

// SQLite Storage Implementation
export * from "./sqlite/index.js";

// Postgres Storage Implementation
export * from "./postgres/index.js";

// In-Memory Storage Implementation (for testing)
export * from "./memory/index.js";

// File Note Storage (replaces deprecated JsonNoteStorage)
export { FileNoteStorage, type NoteEntry } from "./json/json-note-storage.js";