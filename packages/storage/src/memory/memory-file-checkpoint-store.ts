/**
 * In-Memory File Checkpoint Store Adapter
 * Fast, ephemeral storage for file checkpoints, primarily for testing
 */

import type { FileCheckpointMetadata, FileCheckpointListOptions } from "@wf-agent/types";
import type { FileCheckpointStorageAdapter } from "../types/adapter/file-checkpoint-adapter.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("memory-file-checkpoint-store");

export class MemoryFileCheckpointStore implements FileCheckpointStorageAdapter {
  private metadataStore = new Map<string, FileCheckpointMetadata>();
  private filesStore = new Map<string, Map<string, Buffer>>();
  private initialized = false;

  async initialize(): Promise<void> {
    this.initialized = true;
    logger.info("MemoryFileCheckpointStore initialized");
  }

  async close(): Promise<void> {
    this.metadataStore.clear();
    this.filesStore.clear();
    this.initialized = false;
    logger.info("MemoryFileCheckpointStore closed");
  }

  async clear(): Promise<void> {
    this.metadataStore.clear();
    this.filesStore.clear();
    logger.info("MemoryFileCheckpointStore cleared");
  }

  async save(
    id: string,
    metadata: FileCheckpointMetadata,
    files: Map<string, Buffer>,
  ): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    this.metadataStore.set(id, { ...metadata });
    this.filesStore.set(id, new Map(files));
    logger.debug("File checkpoint saved", { checkpointId: id, fileCount: files.size });
  }

  async load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null> {
    if (!this.initialized) {
      await this.initialize();
    }

    const metadata = this.metadataStore.get(id);
    if (!metadata) return null;

    const files = this.filesStore.get(id);
    return {
      metadata: { ...metadata },
      files: files ? new Map(files) : new Map(),
    };
  }

  async delete(id: string): Promise<void> {
    this.metadataStore.delete(id);
    this.filesStore.delete(id);
    logger.debug("File checkpoint deleted", { checkpointId: id });
  }

  async list(options?: FileCheckpointListOptions): Promise<string[]> {
    if (!this.initialized) {
      await this.initialize();
    }

    let ids = Array.from(this.metadataStore.keys());

    // Apply filters
    if (options) {
      if (options.entityId) {
        ids = ids.filter(id => this.metadataStore.get(id)?.entityId === options.entityId);
      }
      if (options.type) {
        ids = ids.filter(id => this.metadataStore.get(id)?.type === options.type);
      }
      if (options.timestampFrom !== undefined) {
        ids = ids.filter(id => (this.metadataStore.get(id)?.timestamp ?? 0) >= options.timestampFrom!);
      }
      if (options.timestampTo !== undefined) {
        ids = ids.filter(id => (this.metadataStore.get(id)?.timestamp ?? 0) <= options.timestampTo!);
      }
    }

    // Sort by timestamp descending
    ids.sort((a, b) => (this.metadataStore.get(b)?.timestamp ?? 0) - (this.metadataStore.get(a)?.timestamp ?? 0));

    // Pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? ids.length;
      ids = ids.slice(offset, offset + limit);
    }

    return ids;
  }

  async listByEntity(
    entityId: string,
    options?: { limit?: number },
  ): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>> {
    if (!this.initialized) {
      await this.initialize();
    }

    const items = Array.from(this.metadataStore.entries())
      .filter(([_, meta]) => meta.entityId === entityId)
      .map(([id, meta]) => ({ id, metadata: { ...meta } }))
      .sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    if (options?.limit) {
      return items.slice(0, options.limit);
    }
    return items;
  }

  async getLatestByEntity(
    entityId: string,
  ): Promise<{ id: string; metadata: FileCheckpointMetadata; files?: Map<string, Buffer> } | null> {
    const items = await this.listByEntity(entityId, { limit: 1 });
    if (items.length === 0) return null;

    const item = items[0]!;
    const files = this.filesStore.get(item.id);
    return {
      ...item,
      files: files ? new Map(files) : undefined,
    };
  }

  async deleteByEntity(entityId: string, keepLatest?: number): Promise<number> {
    const allForEntity = Array.from(this.metadataStore.entries())
      .filter(([_, meta]) => meta.entityId === entityId)
      .sort((a, b) => b[1].timestamp - a[1].timestamp);

    let toDelete: string[];

    if (keepLatest && keepLatest > 0) {
      const keepIds = new Set(allForEntity.slice(0, keepLatest).map(([id]) => id));
      toDelete = allForEntity.filter(([id]) => !keepIds.has(id)).map(([id]) => id);
    } else {
      toDelete = allForEntity.map(([id]) => id);
    }

    for (const id of toDelete) {
      this.metadataStore.delete(id);
      this.filesStore.delete(id);
    }

    logger.info("Deleted file checkpoints by entity", { entityId, count: toDelete.length, keepLatest });
    return toDelete.length;
  }
}
