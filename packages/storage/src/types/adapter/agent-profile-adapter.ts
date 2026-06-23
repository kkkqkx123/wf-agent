/**
 * Agent Profile Storage Adapter Interface Definition
 * Define a uniform interface for agent profile persistence operations
 */

import type { AgentProfileStorageMetadata } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Agent Profile Storage Adapter Interface
 *
 * Defines a unified interface for agent profile persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides a AgentProfileStorageAdapter implementation based on this interface.
 * - The application layer can use AgentProfileStorageAdapter directly or implement it by itself.
 */
export type AgentProfileStorageAdapter = BaseStorageAdapter<AgentProfileStorageMetadata, void>;