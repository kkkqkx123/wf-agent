/**
 * In-Memory Workflow Storage Adapter
 * Fast, isolated workflow storage for testing
 */

import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
} from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "../types/adapter/workflow-adapter.js";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

interface VersionEntry {
  version: string;
  data: Uint8Array;
  changeNote?: string;
  createdAt: number;
}

/**
 * Memory-based workflow storage implementation
 * Implements WorkflowStorageAdapter interface with in-memory storage
 */
export class MemoryWorkflowStorage
  extends BaseMemoryStorage<WorkflowStorageMetadata, WorkflowListOptions>
  implements WorkflowStorageAdapter
{
  private versionStore: Map<string, VersionEntry[]> = new Map();

  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * Update workflow metadata
   */
  async updateWorkflowMetadata(
    workflowId: string,
    metadata: Partial<WorkflowStorageMetadata>,
  ): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const entry = this.store.get(workflowId);
    if (!entry) {
      throw new Error(`Workflow not found: ${workflowId}`);
    }

    // Merge metadata
    entry.metadata = { ...entry.metadata, ...metadata };
    this.store.set(workflowId, entry);
  }

  /**
   * Save workflow version
   */
  async saveWorkflowVersion(
    workflowId: string,
    version: string,
    data: Uint8Array,
    changeNote?: string,
  ): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const versions = this.versionStore.get(workflowId) || [];

    // Check if version already exists and update it
    const existingIndex = versions.findIndex(v => v.version === version);
    if (existingIndex >= 0) {
      versions[existingIndex] = {
        version,
        data: new Uint8Array(data),
        changeNote,
        createdAt: Date.now(),
      };
    } else {
      versions.push({
        version,
        data: new Uint8Array(data),
        changeNote,
        createdAt: Date.now(),
      });
    }

    this.versionStore.set(workflowId, versions);
  }

  /**
   * List workflow versions
   */
  async listWorkflowVersions(
    workflowId: string,
    options?: WorkflowVersionListOptions,
  ): Promise<WorkflowVersionInfo[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    const versions = this.versionStore.get(workflowId) || [];

    let result = versions.map(v => ({
      version: v.version,
      createdAt: v.createdAt,
      changeNote: v.changeNote,
      isCurrent: false, // Would need to track current version separately
    }));

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? result.length;
      result = result.slice(offset, offset + limit);
    }

    return result;
  }

  /**
   * Load specified workflow version
   */
  async loadWorkflowVersion(workflowId: string, version: string): Promise<Uint8Array | null> {
    this.ensureInitialized();
    await this.simulateLatency();

    const versions = this.versionStore.get(workflowId);
    if (!versions) {
      return null;
    }

    const versionEntry = versions.find(v => v.version === version);
    return versionEntry ? new Uint8Array(versionEntry.data) : null;
  }

  /**
   * Delete workflow version
   */
  async deleteWorkflowVersion(workflowId: string, version: string): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const versions = this.versionStore.get(workflowId);
    if (!versions) {
      return;
    }

    const filtered = versions.filter(v => v.version !== version);
    if (filtered.length === 0) {
      this.versionStore.delete(workflowId);
    } else {
      this.versionStore.set(workflowId, filtered);
    }
  }

  /**
   * List workflow IDs with advanced filtering support
   */
  override async list(options?: WorkflowListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.name) {
        const nameFilter = options.name.toLowerCase();
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.name.toLowerCase().includes(nameFilter);
        });
      }

      if (options.author) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.author === options.author;
        });
      }

      if (options.category) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.category === options.category;
        });
      }

      if (options.tags && options.tags.length > 0) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          const metadataTags = entry?.metadata.tags || [];
          return options.tags!.some(tag => metadataTags.includes(tag));
        });
      }

      if (options.enabled !== undefined) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.enabled === options.enabled;
        });
      }

      if (options.createdAtFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.createdAt >= options.createdAtFrom!;
        });
      }

      if (options.createdAtTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.createdAt <= options.createdAtTo!;
        });
      }

      if (options.updatedAtFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.updatedAt >= options.updatedAtFrom!;
        });
      }

      if (options.updatedAtTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.updatedAt <= options.updatedAtTo!;
        });
      }

      // Apply sorting
      if (options.sortBy) {
        ids.sort((a, b) => {
          const entryA = this.store.get(a);
          const entryB = this.store.get(b);

          if (!entryA || !entryB) return 0;

          let comparison = 0;
          switch (options.sortBy) {
            case "name":
              comparison = entryA.metadata.name.localeCompare(entryB.metadata.name);
              break;
            case "createdAt":
              comparison = entryA.metadata.createdAt - entryB.metadata.createdAt;
              break;
            case "updatedAt":
              comparison = entryA.metadata.updatedAt - entryB.metadata.updatedAt;
              break;
          }

          return options.sortOrder === "desc" ? -comparison : comparison;
        });
      }
    }

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? ids.length;
      ids = ids.slice(offset, offset + limit);
    }

    return ids;
  }
}
