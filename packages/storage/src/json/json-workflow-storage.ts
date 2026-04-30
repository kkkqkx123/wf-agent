/**
 * JSON File Workflow Storage Implementation
 * Workflow persistence storage based on JSON file system with metadata-data separation
 */

import * as fs from "fs/promises";
import * as path from "path";
import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
} from "@wf-agent/types";
import type { WorkflowStorageCallback } from "../types/callback/index.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";
import { StorageError, SerializationError } from "../types/storage-errors.js";
import { compressBlob, decompressBlob, DEFAULT_COMPRESSION_CONFIG } from "../compression/index.js";
import { createHash } from "crypto";

/**
 * Workflow version metadata file content
 */
interface VersionMetadataContent {
  workflowId: string;
  version: string;
  changeNote?: string;
  createdAt: number;
  dataRef: {
    filePath: string;
    size: number;
    hash: string;
    compressed: boolean;
    compressionAlgorithm?: string;
  };
}

/**
 * JSON File Workflow Storage
 * Implements the WorkflowStorageCallback interface
 */
export class JsonWorkflowStorage
  extends BaseJsonStorage<WorkflowStorageMetadata>
  implements WorkflowStorageCallback
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for workflows
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "workflow");
  }

  /**
   * Get data directory path for workflows
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "workflow");
  }

  /**
   * Get versions metadata directory
   */
  protected getVersionsMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "versions");
  }

  /**
   * Get versions data directory
   */
  protected getVersionsDataDir(): string {
    return path.join(this.config.baseDir, "data", "versions");
  }

  /**
   * Initialize storage
   * Creates directory structure including version directories
   */
  override async initialize(): Promise<void> {
    await fs.mkdir(this.config.baseDir, { recursive: true });
    await fs.mkdir(this.getMetadataDir(), { recursive: true });
    await fs.mkdir(this.getDataDir(), { recursive: true });
    await fs.mkdir(this.getVersionsMetadataDir(), { recursive: true });
    await fs.mkdir(this.getVersionsDataDir(), { recursive: true });

    await this.loadMetadataIndex();
    this["initialized"] = true;
  }

  /**
   * Get version metadata file path
   */
  private getVersionMetadataPath(workflowId: string, version: string): string {
    const safeId = this["sanitizeId"](workflowId);
    const safeVersion = this["sanitizeId"](version);
    return path.join(this.getVersionsMetadataDir(), `${safeId}_${safeVersion}.json`);
  }

  /**
   * Get version data file path
   */
  private getVersionDataPath(workflowId: string, version: string): string {
    const safeId = this["sanitizeId"](workflowId);
    const safeVersion = this["sanitizeId"](version);
    return path.join(this.getVersionsDataDir(), `${safeId}_${safeVersion}.bin`);
  }

  /**
   * Compute hash for data integrity
   */
  protected override computeHash(data: Uint8Array): string {
    return createHash("sha256").update(data).digest("hex").substring(0, 16);
  }

  /**
   * Delete workflow and its versions
   */
  override async delete(id: string): Promise<void> {
    this["ensureInitialized"]();

    const indexEntry = this["metadataIndex"].get(id);
    if (!indexEntry) {
      return;
    }

    const releaseLock = await this["acquireLock"](indexEntry.metadataPath);

    try {
      // Delete workflow files
      await fs.unlink(indexEntry.metadataPath).catch(e => {
        if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
      });
      await fs.unlink(indexEntry.dataPath).catch(e => {
        if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
      });

      this["metadataIndex"].delete(id);

      // Delete all versions
      const safeId = this["sanitizeId"](id);
      const versionMetaFiles = await fs.readdir(this.getVersionsMetadataDir()).catch(() => []);
      const versionDataFiles = await fs.readdir(this.getVersionsDataDir()).catch(() => []);

      for (const file of versionMetaFiles) {
        if (file.startsWith(`${safeId}_`) && file.endsWith(".json")) {
          await fs.unlink(path.join(this.getVersionsMetadataDir(), file)).catch(() => {});
        }
      }

      for (const file of versionDataFiles) {
        if (file.startsWith(`${safeId}_`) && file.endsWith(".bin")) {
          await fs.unlink(path.join(this.getVersionsDataDir(), file)).catch(() => {});
        }
      }
    } finally {
      releaseLock();
    }
  }

  /**
   * List workflow IDs
   */
  async list(options?: WorkflowListOptions): Promise<string[]> {
    this["ensureInitialized"]();

    let ids = this.getAllIds();

    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.name && !metadata.name.toLowerCase().includes(options.name.toLowerCase())) {
          return false;
        }

        if (options.author && metadata.author !== options.author) {
          return false;
        }

        if (options.category && metadata.category !== options.category) {
          return false;
        }

        if (options.enabled !== undefined && metadata.enabled !== options.enabled) {
          return false;
        }

        if (options.createdAtFrom && metadata.createdAt < options.createdAtFrom) {
          return false;
        }

        if (options.createdAtTo && metadata.createdAt > options.createdAtTo) {
          return false;
        }

        if (options.updatedAtFrom && metadata.updatedAt < options.updatedAtFrom) {
          return false;
        }

        if (options.updatedAtTo && metadata.updatedAt > options.updatedAtTo) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        return true;
      });
    }

    const sortBy = options?.sortBy ?? "updatedAt";
    const sortOrder = options?.sortOrder ?? "desc";

    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;

      let valueA: number | string;
      let valueB: number | string;

      switch (sortBy) {
        case "name":
          valueA = metaA?.name ?? "";
          valueB = metaB?.name ?? "";
          return sortOrder === "asc"
            ? (valueA as string).localeCompare(valueB as string)
            : (valueB as string).localeCompare(valueA as string);
        case "createdAt":
          valueA = metaA?.createdAt ?? 0;
          valueB = metaB?.createdAt ?? 0;
          break;
        case "updatedAt":
        default:
          valueA = metaA?.updatedAt ?? 0;
          valueB = metaB?.updatedAt ?? 0;
          break;
      }

      return sortOrder === "asc"
        ? (valueA as number) - (valueB as number)
        : (valueB as number) - (valueA as number);
    });

    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }

  /**
   * Update workflow metadata
   */
  async updateWorkflowMetadata(
    workflowId: string,
    metadata: Partial<WorkflowStorageMetadata>,
  ): Promise<void> {
    this["ensureInitialized"]();

    const indexEntry = this["metadataIndex"].get(workflowId);
    if (!indexEntry) {
      throw new StorageError(`Workflow not found: ${workflowId}`, "updateMetadata", { workflowId });
    }

    const updatedMetadata: WorkflowStorageMetadata = {
      ...indexEntry.metadata,
      ...metadata,
      updatedAt: Date.now(),
    };

    const data = await this.load(workflowId);
    if (data) {
      await this.save(workflowId, data, updatedMetadata);
    }
  }

  // ==================== Version Management ====================

  /**
   * Save workflow version with metadata-data separation
   */
  async saveWorkflowVersion(
    workflowId: string,
    version: string,
    data: Uint8Array,
    changeNote?: string,
  ): Promise<void> {
    this["ensureInitialized"]();

    const metadataPath = this.getVersionMetadataPath(workflowId, version);
    const dataPath = this.getVersionDataPath(workflowId, version);

    const releaseLock = await this["acquireLock"](metadataPath);

    try {
      // Compress data
      const compressionConfig = this.config.compression ?? DEFAULT_COMPRESSION_CONFIG;
      const compressionResult = await compressBlob(data, compressionConfig);

      const dataToWrite = compressionResult.compressed;
      const dataHash = this.computeHash(data);

      // Write data file
      await fs.writeFile(dataPath, Buffer.from(dataToWrite));

      // Write metadata file
      const content: VersionMetadataContent = {
        workflowId,
        version,
        changeNote,
        createdAt: Date.now(),
        dataRef: {
          filePath: path.relative(this.config.baseDir, dataPath),
          size: data.length,
          hash: dataHash,
          compressed: compressionResult.algorithm !== null,
          compressionAlgorithm: compressionResult.algorithm ?? undefined,
        },
      };

      await fs.writeFile(metadataPath, JSON.stringify(content, null, 2), "utf-8");
    } catch (error) {
      throw new SerializationError(
        `Failed to save workflow version: ${workflowId}@${version}`,
        workflowId,
        error as Error,
      );
    } finally {
      releaseLock();
    }
  }

  /**
   * List workflow versions
   */
  async listWorkflowVersions(
    workflowId: string,
    options?: WorkflowVersionListOptions,
  ): Promise<WorkflowVersionInfo[]> {
    this["ensureInitialized"]();

    const safeId = this["sanitizeId"](workflowId);
    const versions: WorkflowVersionInfo[] = [];

    try {
      const files = await fs.readdir(this.getVersionsMetadataDir());
      const currentMetadata = this["metadataIndex"].get(workflowId);
      const currentVersion = currentMetadata?.metadata.version;

      for (const file of files) {
        if (file.startsWith(`${safeId}_`) && file.endsWith(".json")) {
          try {
            const content = await fs.readFile(
              path.join(this.getVersionsMetadataDir(), file),
              "utf-8",
            );
            const parsed = JSON.parse(content) as VersionMetadataContent;
            versions.push({
              version: parsed.version,
              createdAt: parsed.createdAt,
              changeNote: parsed.changeNote,
              isCurrent: parsed.version === currentVersion,
            });
          } catch {
            // Ignore parsing errors
          }
        }
      }

      // Sort by creation time descending
      versions.sort((a, b) => b.createdAt - a.createdAt);

      const offset = options?.offset ?? 0;
      const limit = options?.limit ?? versions.length;
      return versions.slice(offset, offset + limit);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return [];
      }
      throw new StorageError(
        `Failed to list workflow versions: ${workflowId}`,
        "listVersions",
        { workflowId },
        error as Error,
      );
    }
  }

  /**
   * Load workflow version
   */
  async loadWorkflowVersion(workflowId: string, version: string): Promise<Uint8Array | null> {
    this["ensureInitialized"]();

    const metadataPath = this.getVersionMetadataPath(workflowId, version);

    try {
      const metaContent = await fs.readFile(metadataPath, "utf-8");
      const meta = JSON.parse(metaContent) as VersionMetadataContent;

      const dataPath = path.join(this.config.baseDir, meta.dataRef.filePath);
      const buffer = await fs.readFile(dataPath);
      let data = new Uint8Array(buffer);

      // Decompress if needed
      if (meta.dataRef.compressed && meta.dataRef.compressionAlgorithm) {
        data = new Uint8Array(await decompressBlob(data, meta.dataRef.compressionAlgorithm));
      }

      return data;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return null;
      }
      throw new StorageError(
        `Failed to load workflow version: ${workflowId}@${version}`,
        "loadVersion",
        { workflowId, version },
        error as Error,
      );
    }
  }

  /**
   * Delete workflow version
   */
  async deleteWorkflowVersion(workflowId: string, version: string): Promise<void> {
    this["ensureInitialized"]();

    const metadataPath = this.getVersionMetadataPath(workflowId, version);
    const dataPath = this.getVersionDataPath(workflowId, version);

    try {
      await fs.unlink(metadataPath).catch(e => {
        if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
      });
      await fs.unlink(dataPath).catch(e => {
        if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
      });
    } catch (error) {
      throw new StorageError(
        `Failed to delete workflow version: ${workflowId}@${version}`,
        "deleteVersion",
        { workflowId, version },
        error as Error,
      );
    }
  }

  /**
   * Clear all data including versions
   */
  override async clear(): Promise<void> {
    this["ensureInitialized"]();

    // Clear main storage
    for (const [, entry] of this["metadataIndex"]) {
      await fs.unlink(entry.metadataPath).catch(() => {});
      await fs.unlink(entry.dataPath).catch(() => {});
    }
    this["metadataIndex"].clear();

    // Clear versions
    const versionMetaFiles = await fs.readdir(this.getVersionsMetadataDir()).catch(() => []);
    const versionDataFiles = await fs.readdir(this.getVersionsDataDir()).catch(() => []);

    for (const file of versionMetaFiles) {
      await fs.unlink(path.join(this.getVersionsMetadataDir(), file)).catch(() => {});
    }

    for (const file of versionDataFiles) {
      await fs.unlink(path.join(this.getVersionsDataDir(), file)).catch(() => {});
    }
  }
}
