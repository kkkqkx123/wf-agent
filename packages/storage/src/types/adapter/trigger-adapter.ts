/**
 * Trigger Storage Adapter Interface Definition
 * Define a uniform interface for trigger template persistence operations
 */

import type { TriggerStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Trigger Storage Adapter Interface
 *
 * Defines a unified interface for trigger template persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a TriggerStorageAdapter implementation based on this interface.
 * - The application layer can use TriggerStorageAdapter directly or implement it by itself.
 */
export type TriggerStorageAdapter = BaseStorageAdapter<TriggerStorageMetadata, void>;