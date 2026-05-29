/**
 * JSON File Checkpoint Store Implementation
 * Stores file checkpoints as individual directories with metadata.json and file contents
 *
 * Directory structure:
 *   {baseDir}/file-checkpoints/
 *     {checkpointId}/
 *       metadata.json         -- checkpoint metadata
 *       files/                -- file contents stored as individual files
 *         {encoded-path}     -- encoded file path (safe for filesystem)
 *         ...
 */

import * as fs from "fs/promises";
import * as path from "path";
import { createModuleLogger } from "../logger.js";
import type { FileCheckpointMetadata, FileCheckpointListOptions } from "@wf-agent/types";
import type { FileCheckpointStorageAdapter } from "../types/adapter/file-checkpoint-adapter.js";

const logger = createModuleLogger("json-file-checkpoint-store");

export interface JsonFileCheckpointStoreConfig {
  /** Base directory for file checkpoint data */
  baseDir: string;
}

/**
 * Encode a file path to a filesystem-safe name
 * Replaces path separators and special characters
 */
function encodeFilePath(filePath: string): string {
  return Buffer.from(filePath, "utf-8").toString("base64url").replace(/=/g, "");
}

/**
 * Decode a filesystem-safe name back to a file path
 */
function decodeFilePath(encoded: string): string {
  // Add padding if needed
  const padded = encoded.padEnd(Math.ceil(encoded.length / 4) * 4, "=");
  return Buffer.from(padded, "base64url").toString("utf-8");
}

export class JsonFileCheckpointStore implements FileCheckpointStorageAdapter {
  private baseDir: string;
  private initialized = false;

  constructor(config: JsonFileCheckpointStoreConfig) {
    this.baseDir = config.baseDir;
  }

  /**
   * Get the checkpoint directory path
   */
  private getCheckpointDir(id: string): string {
    return path.join(this.baseDir, "file-checkpoints", id);
  }

  /**
   * Get the metadata file path for a checkpoint
   */
  private getMetadataPath(id: string): string {
    return path.join(this.getCheckpointDir(id), "metadata.json");
  }

  /**
   * Get the files directory path for a checkpoint
   */
  private getFilesDir(id: string): string {
    return path.join(this.getCheckpointDir(id), "files");
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      await fs.mkdir(this.baseDir, { recursive: true });
      this.initialized = true;
      logger.info("JsonFileCheckpointStore initialized", { baseDir: this.baseDir });
    } catch (error) {
      logger.error("Failed to initialize JsonFileCheckpointStore", { error });
      throw error;
    }
  }

  async close(): Promise<void> {
    this.initialized = false;
    logger.info("JsonFileCheckpointStore closed");
  }

  async clear(): Promise<void> {
    const cpDir = path.join(this.baseDir, "file-checkpoints");
    try {
      await fs.rm(cpDir, { recursive: true, force: true });
      await fs.mkdir(cpDir, { recursive: true });
      logger.info("JsonFileCheckpointStore cleared");
    } catch (error) {
      logger.error("Failed to clear JsonFileCheckpointStore", { error });
      throw error;
    }
  }

  async save(
    id: string,
    metadata: FileCheckpointMetadata,
    files: Map<string, Buffer>,
  ): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    const checkpointDir = this.getCheckpointDir(id);
    const filesDir = this.getFilesDir(id);

    try {
      await fs.mkdir(filesDir, { recursive: true });

      // Write metadata.json
      const metadataContent = {
        ...metadata,
        emptyDirs: metadata.emptyDirs || [],
        fileHashSnapshot: metadata.fileHashSnapshot || {},
        changes: metadata.changes || [],
      };
      await fs.writeFile(
        this.getMetadataPath(id),
        JSON.stringify(metadataContent, null, 2),
        "utf-8",
      );

      // Write each file
      for (const [filePath, content] of files) {
        const encodedName = encodeFilePath(filePath);
        const fileDest = path.join(filesDir, encodedName);
        await fs.mkdir(path.dirname(fileDest), { recursive: true });
        await fs.writeFile(fileDest, content);
      }

      logger.debug("File checkpoint saved", {
        checkpointId: id,
        fileCount: files.size,
        dir: checkpointDir,
      });
    } catch (error) {
      logger.error("Failed to save file checkpoint", { checkpointId: id, error });
      throw error;
    }
  }

  async load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null> {
    const metadataPath = this.getMetadataPath(id);
    const filesDir = this.getFilesDir(id);

    try {
      // Load metadata
      const metadataContent = await fs.readFile(metadataPath, "utf-8");
      const metadata: FileCheckpointMetadata = JSON.parse(metadataContent);

      // Load files
      const files = new Map<string, Buffer>();
      try {
        const entries = await fs.readdir(filesDir, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.isFile()) {
            const filePath = decodeFilePath(entry.name);
            const content = await fs.readFile(path.join(filesDir, entry.name));
            files.set(filePath, content);
          }
        }
      } catch {
        // Files directory may not exist
      }

      return { metadata, files };
    } catch {
      return null;
    }
  }

  async delete(id: string): Promise<void> {
    const checkpointDir = this.getCheckpointDir(id);
    try {
      await fs.rm(checkpointDir, { recursive: true, force: true });
      logger.debug("File checkpoint deleted", { checkpointId: id });
    } catch (error) {
      logger.error("Failed to delete file checkpoint", { checkpointId: id, error });
      throw error;
    }
  }

  async list(options?: FileCheckpointListOptions): Promise<string[]> {
    const cpDir = path.join(this.baseDir, "file-checkpoints");

    try {
      const entries = await fs.readdir(cpDir, { withFileTypes: true });
      let ids = entries.filter(e => e.isDirectory()).map(e => e.name);

      // Filter by metadata if needed
      if (options && (options.entityId || options.type || options.timestampFrom !== undefined || options.timestampTo !== undefined)) {
        const filtered: Array<{ id: string; timestamp: number }> = [];

        for (const id of ids) {
          try {
            const metaContent = await fs.readFile(this.getMetadataPath(id), "utf-8");
            const meta: FileCheckpointMetadata = JSON.parse(metaContent);

            if (options.entityId && meta.entityId !== options.entityId) continue;
            if (options.type && meta.type !== options.type) continue;
            if (options.timestampFrom !== undefined && meta.timestamp < options.timestampFrom!) continue;
            if (options.timestampTo !== undefined && meta.timestamp > options.timestampTo!) continue;

            filtered.push({ id, timestamp: meta.timestamp });
          } catch {
            // Skip corrupted checkpoints
            continue;
          }
        }

        // Sort by timestamp descending
        filtered.sort((a, b) => b.timestamp - a.timestamp);

        ids = filtered.map(f => f.id);

        // Pagination after filtering
        if (options.offset !== undefined || options.limit !== undefined) {
          const offset = options.offset ?? 0;
          const limit = options.limit ?? ids.length;
          ids = ids.slice(offset, offset + limit);
        }
      } else {
        // No filters, just sort by directory name (which is the id)
        ids.sort();

        // Pagination
        if (options?.offset !== undefined || options?.limit !== undefined) {
          const offset = options.offset ?? 0;
          const limit = options.limit ?? ids.length;
          ids = ids.slice(offset, offset + limit);
        }
      }

      return ids;
    } catch {
      // Directory may not exist
      return [];
    }
  }

  async listByEntity(
    entityId: string,
    options?: { limit?: number },
  ): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>> {
    const allIds = await this.list({ entityId });
    const items: Array<{ id: string; metadata: FileCheckpointMetadata }> = [];

    for (const id of allIds) {
      try {
        const metaContent = await fs.readFile(this.getMetadataPath(id), "utf-8");
        const metadata: FileCheckpointMetadata = JSON.parse(metaContent);
        items.push({ id, metadata });
      } catch {
        continue;
      }
    }

    // Sort by timestamp descending
    items.sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

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

    const item = items[0];
    if (!item) return null;
    const loaded = await this.load(item.id);
    return {
      id: item.id,
      metadata: item.metadata,
      files: loaded?.files,
    };
  }

  async deleteByEntity(entityId: string, keepLatest?: number): Promise<number> {
    const allForEntity = await this.listByEntity(entityId);
    allForEntity.sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    let toDelete: Array<{ id: string }>;

    if (keepLatest && keepLatest > 0) {
      const keepIds = new Set(allForEntity.slice(0, keepLatest).map(i => i.id));
      toDelete = allForEntity.filter(i => !keepIds.has(i.id));
    } else {
      toDelete = allForEntity;
    }

    for (const item of toDelete) {
      await this.delete(item.id);
    }

    logger.info("Deleted file checkpoints by entity", {
      entityId,
      count: toDelete.length,
      keepLatest,
    });
    return toDelete.length;
  }
}
