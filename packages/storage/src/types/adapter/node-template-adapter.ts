/**
 * Node Template Storage Adapter Interface Definition
 * Define a uniform interface for node template persistence operations
 */

import type { NodeTemplateStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Node Template Storage Adapter Interface
 *
 * Defines a unified interface for node template persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a NodeTemplateStorageAdapter implementation based on this interface.
 * - The application layer can use NodeTemplateStorageAdapter directly or implement it by itself.
 */
export type NodeTemplateStorageAdapter = BaseStorageAdapter<NodeTemplateStorageMetadata, void>;