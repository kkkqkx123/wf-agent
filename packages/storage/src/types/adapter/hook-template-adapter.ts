/**
 * Hook Template Storage Adapter Interface Definition
 * Define a uniform interface for hook template persistence operations
 */

import type { HookTemplateStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Hook Template Storage Adapter Interface
 *
 * Defines a unified interface for hook template persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a HookTemplateStorageAdapter implementation based on this interface.
 * - The application layer can use HookTemplateStorageAdapter directly or implement it by itself.
 */
export type HookTemplateStorageAdapter = BaseStorageAdapter<HookTemplateStorageMetadata, void>;