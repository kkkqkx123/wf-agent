/**
 * JSON File Storage Base Class with Metadata-Data Separation
 * Separates metadata (JSON) and binary data (raw files) for better performance
 */

import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { StorageError, SerializationError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";
import {
  compressBlob,
  decompressBlob,
  CompressionConfig,
  DEFAULT_COMPRESSION_CONFIG,
} from "../compression/index.js";

const logger = createModuleLogger("json-storage");

/**
 * JSON Storage Configuration
 */
export interface BaseJsonStorageConfig {
  /** Base directory path */
  baseDir: string;
  /** Whether to enable file locking */
  enableFileLock?: boolean;
  /** Compression configuration */
  compression?: CompressionConfig;
}

/**
 * Data reference in metadata file
 */
interface DataReference {
  /** Data file path (relative to baseDir) */
  filePath: string;
  /** Data size in bytes */
  size: number;
  /** Data hash for integrity check */
  hash: string;
  /** Whether data is compressed */
  compressed: boolean;
  /** Compression algorithm used */
  compressionAlgorithm?: string;
}

/**
 * Metadata file content format
 */
interface MetadataFileContent<TMetadata> {
  id: string;
  metadata: TMetadata;
  dataRef: DataReference;
}

/**
 * Metadata index entry
 */
interface MetadataIndexEntry<TMetadata> {
  metadata: TMetadata;
  metadataPath: string;
  dataPath: string;
  dataRef: DataReference;
}

/**
 * JSON File Storage Abstract Base Class
 * Separates metadata (JSON) and binary data for optimal performance
 * @template TMetadata Metadata Type
 */
export abstract class BaseJsonStorage<TMetadata> {
  protected metadataIndex: Map<string, MetadataIndexEntry<TMetadata>> = new Map();
  protected initialized: boolean = false;
  protected lockFiles: Map<string, Promise<void>> = new Map();

  constructor(protected readonly config: BaseJsonStorageConfig) {}

  /**
   * Get metadata directory path
   */
  protected getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata");
  }

  /**
   * Get data directory path
   */
  protected getDataDir(): string {
    return path.join(this.config.baseDir, "data");
  }

  /**
   * Get metadata file path
   */
  protected getMetadataFilePath(id: string): string {
    const safeId = this.sanitizeId(id);
    return path.join(this.getMetadataDir(), `${safeId}.json`);
  }

  /**
   * Get data file path
   */
  protected getDataFilePath(id: string): string {
    const safeId = this.sanitizeId(id);
    return path.join(this.getDataDir(), `${safeId}.bin`);
  }

  /**
   * Initialize storage
   * Create directory structure and load metadata index
   */
  async initialize(): Promise<void> {
    logger.debug("Initializing JSON storage", { baseDir: this.config.baseDir });

    // Create directories
    await fs.mkdir(this.config.baseDir, { recursive: true });
    await fs.mkdir(this.getMetadataDir(), { recursive: true });
    await fs.mkdir(this.getDataDir(), { recursive: true });

    await this.loadMetadataIndex();
    this.initialized = true;

    logger.info("JSON storage initialized", {
      baseDir: this.config.baseDir,
      indexSize: this.metadataIndex.size,
    });
  }

  /**
   * Load metadata index
   * Only reads metadata files, not data files
   */
  protected async loadMetadataIndex(): Promise<void> {
    try {
      const entries = await fs.readdir(this.getMetadataDir(), { withFileTypes: true });

      for (const entry of entries) {
        if (entry.isFile() && entry.name.endsWith(".json")) {
          try {
            const metadataPath = path.join(this.getMetadataDir(), entry.name);
            const content = await fs.readFile(metadataPath, "utf-8");
            const parsed = JSON.parse(content) as MetadataFileContent<TMetadata>;

            if (parsed.id && parsed.metadata && parsed.dataRef) {
              const dataPath = path.join(this.config.baseDir, parsed.dataRef.filePath);
              this.metadataIndex.set(parsed.id, {
                metadata: parsed.metadata,
                metadataPath,
                dataPath,
                dataRef: parsed.dataRef,
              });
            }
          } catch {
            // Ignore files with parsing errors
          }
        }
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
    }
  }

  /**
   * Ensure storage is initialized
   */
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }

  /**
   * Sanitize ID to prevent path traversal
   */
  protected sanitizeId(id: string): string {
    return id.replace(/[/\\:*?"<>|]/g, "_");
  }

  /**
   * Compute hash for data integrity check
   */
  protected computeHash(data: Uint8Array): string {
    return createHash("sha256").update(data).digest("hex").substring(0, 16);
  }

  /**
   * Acquire file lock
   */
  protected async acquireLock(filePath: string): Promise<() => void> {
    if (!this.config.enableFileLock) {
      return () => {};
    }

    while (this.lockFiles.has(filePath)) {
      await this.lockFiles.get(filePath);
    }

    let releaseLock: () => void;
    const lockPromise = new Promise<void>(resolve => {
      releaseLock = resolve;
    });

    this.lockFiles.set(filePath, lockPromise);

    return () => {
      this.lockFiles.delete(filePath);
      releaseLock!();
    };
  }

  /**
   * Get compression config
   */
  protected getCompressionConfig(): CompressionConfig {
    return this.config.compression ?? DEFAULT_COMPRESSION_CONFIG;
  }

  /**
   * Save data to storage
   * Writes metadata to JSON file and data to binary file
   */
  async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void> {
    this.ensureInitialized();

    const metadataPath = this.getMetadataFilePath(id);
    const dataPath = this.getDataFilePath(id);

    // Use metadata file for locking
    const releaseLock = await this.acquireLock(metadataPath);

    logger.debug("Saving data to JSON storage", {
      id,
      metadataPath,
      dataPath,
      dataSize: data.length,
    });

    try {
      // Compress data if enabled
      const compressionConfig = this.getCompressionConfig();
      const compressionResult = await compressBlob(data, compressionConfig);

      const dataToWrite = compressionResult.compressed;
      const dataHash = this.computeHash(data);

      // Write data file
      await fs.writeFile(dataPath, Buffer.from(dataToWrite));

      // Create data reference
      const dataRef: DataReference = {
        filePath: path.relative(this.config.baseDir, dataPath),
        size: data.length, // Original size
        hash: dataHash,
        compressed: compressionResult.algorithm !== null,
        compressionAlgorithm: compressionResult.algorithm ?? undefined,
      };

      // Write metadata file
      const content: MetadataFileContent<TMetadata> = {
        id,
        metadata,
        dataRef,
      };

      const jsonContent = JSON.stringify(content, null, 2);
      await fs.writeFile(metadataPath, jsonContent, "utf-8");

      // Update index
      this.metadataIndex.set(id, {
        metadata,
        metadataPath,
        dataPath,
        dataRef,
      });

      logger.debug("Data saved to JSON storage", {
        id,
        compressed: dataRef.compressed,
        compressionRatio: compressionResult.ratio,
      });
    } catch (error) {
      logger.error("Failed to save data to JSON storage", {
        id,
        error: (error as Error).message,
      });
      throw new SerializationError(`Failed to serialize data: ${id}`, id, error as Error);
    } finally {
      releaseLock();
    }
  }

  /**
   * Load data from storage
   * Reads binary data file and decompresses if needed
   */
  async load(id: string): Promise<Uint8Array | null> {
    this.ensureInitialized();

    const indexEntry = this.metadataIndex.get(id);
    if (!indexEntry) {
      logger.debug("Data not found in index", { id });
      return null;
    }

    try {
      // Read binary data file
      const buffer = await fs.readFile(indexEntry.dataPath);
      let data = new Uint8Array(buffer);

      // Decompress if needed
      if (indexEntry.dataRef.compressed && indexEntry.dataRef.compressionAlgorithm) {
        const decompressed = await decompressBlob(data, indexEntry.dataRef.compressionAlgorithm);
        data = new Uint8Array(decompressed);
      }

      logger.debug("Data loaded from JSON storage", {
        id,
        dataSize: data.length,
        compressed: indexEntry.dataRef.compressed,
      });

      return data;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        logger.debug("Data file not found", { id, dataPath: indexEntry.dataPath });
        return null;
      }
      logger.error("Failed to load data from JSON storage", {
        id,
        error: (error as Error).message,
      });
      throw new StorageError(`Failed to load data: ${id}`, "load", { id }, error as Error);
    }
  }

  /**
   * Delete data from storage
   * Deletes both metadata and data files
   */
  async delete(id: string): Promise<void> {
    this.ensureInitialized();

    const indexEntry = this.metadataIndex.get(id);
    if (!indexEntry) {
      logger.debug("Data not found for deletion", { id });
      return;
    }

    const releaseLock = await this.acquireLock(indexEntry.metadataPath);

    try {
      // Delete metadata file
      try {
        await fs.unlink(indexEntry.metadataPath);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }

      // Delete data file
      try {
        await fs.unlink(indexEntry.dataPath);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }

      this.metadataIndex.delete(id);
      logger.debug("Data deleted from JSON storage", { id });
    } finally {
      releaseLock();
    }
  }

  /**
   * Check if data exists
   */
  async exists(id: string): Promise<boolean> {
    this.ensureInitialized();
    return this.metadataIndex.has(id);
  }

  /**
   * Get metadata only (no data loading)
   */
  async getMetadata(id: string): Promise<TMetadata | null> {
    this.ensureInitialized();
    const entry = this.metadataIndex.get(id);
    return entry?.metadata ?? null;
  }

  /**
   * Get data reference info
   */
  async getDataInfo(id: string): Promise<DataReference | null> {
    this.ensureInitialized();
    const entry = this.metadataIndex.get(id);
    return entry?.dataRef ?? null;
  }

  /**
   * Get all IDs
   */
  protected getAllIds(): string[] {
    return Array.from(this.metadataIndex.keys());
  }

  /**
   * Clear all data
   */
  async clear(): Promise<void> {
    this.ensureInitialized();

    logger.debug("Clearing all JSON storage data", { count: this.metadataIndex.size });

    for (const [, entry] of this.metadataIndex) {
      try {
        await fs.unlink(entry.metadataPath);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }
      try {
        await fs.unlink(entry.dataPath);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }
    }

    this.metadataIndex.clear();
    logger.info("JSON storage cleared");
  }

  /**
   * Close storage
   */
  async close(): Promise<void> {
    logger.debug("Closing JSON storage");
    this.metadataIndex.clear();
    this.initialized = false;
    logger.info("JSON storage closed");
  }
}
