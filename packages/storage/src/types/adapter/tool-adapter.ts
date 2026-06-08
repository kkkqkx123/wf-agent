/**
 * Tool Storage Adapter Interface Definition
 * Define a uniform interface for tool persistence operations
 */

import type { ToolStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Tool Storage Adapter Interface
 *
 * Defines a unified interface for tool persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a ToolStorageAdapter implementation based on this interface.
 * - The application layer can use ToolStorageAdapter directly or implement it by itself.
 */
export interface ToolStorageAdapter extends BaseStorageAdapter<ToolStorageMetadata, void> {}