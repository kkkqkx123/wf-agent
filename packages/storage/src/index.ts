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
 * - JSON file storage
 * - SQLite database storage
 */

// Logger Export
export { logger, createModuleLogger } from "./logger.js";

// Core Type Definition
export * from "./types/index.js";

// Compression Utilities (shared across all storage backends)
export * from "./compression/index.js";

// JSON File Storage Implementation
export * from "./json/index.js";

// SQLite Storage Implementation
export * from "./sqlite/index.js";

// In-Memory Storage Implementation (for testing)
export * from "./memory/index.js";

// Utilities
export * from "./utils/index.js";
