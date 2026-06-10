/**
 * Script Storage Adapter Interface Definition
 * Define a uniform interface for script persistence operations
 */

import type { ScriptStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Script Storage Adapter Interface
 *
 * Defines a unified interface for script persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a ScriptStorageAdapter implementation based on this interface.
 * - The application layer can use ScriptStorageAdapter directly or implement it by itself.
 */
export type ScriptStorageAdapter = BaseStorageAdapter<ScriptStorageMetadata, void>;