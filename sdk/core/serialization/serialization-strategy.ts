/**
 * Serialization Strategy Pattern
 * 
 * Provides flexible serialization approaches per entity type.
 * Supports different strategies like JSON+Gzip, MessagePack, Encrypted JSON, etc.
 */

import type { SnapshotBase } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "SerializationStrategy" });

/**
 * Serialization strategy interface
 */
export interface SerializationStrategy {
  /** Strategy name/identifier */
  name: string;
  
  /** Serialize a snapshot to bytes */
  serialize<T extends SnapshotBase>(snapshot: T): Promise<Uint8Array>;
  
  /** Deserialize bytes to a snapshot */
  deserialize<T extends SnapshotBase>(data: Uint8Array): Promise<T>;
  
  /** Whether this strategy supports compression */
  supportsCompression: boolean;
  
  /** Whether this strategy supports encryption */
  supportsEncryption: boolean;
  
  /** Optional description of the strategy */
  description?: string;
}

/**
 * JSON + Gzip serialization strategy (default)
 */
export class JsonGzipStrategy implements SerializationStrategy {
  readonly name = "json-gzip";
  readonly supportsCompression = true;
  readonly supportsEncryption = false;
  readonly description = "JSON serialization with optional gzip compression";

  private readonly prettyPrint: boolean;
  private readonly compressionEnabled: boolean;

  constructor(options?: { prettyPrint?: boolean; compressionEnabled?: boolean }) {
    this.prettyPrint = options?.prettyPrint ?? false;
    this.compressionEnabled = options?.compressionEnabled ?? true;
  }

  async serialize<T extends SnapshotBase>(snapshot: T): Promise<Uint8Array> {
    const json = this.prettyPrint ? JSON.stringify(snapshot, null, 2) : JSON.stringify(snapshot);
    const bytes = new TextEncoder().encode(json);

    if (this.compressionEnabled && bytes.length > 512) {
      try {
        const { compressBlob } = await import("@wf-agent/storage");
        const result = await compressBlob(bytes, {
          enabled: true,
          algorithm: "gzip",
          threshold: 0, // Already checked size
        });
        return result.compressed;
      } catch (error) {
        logger.warn("Compression failed, using uncompressed data", { error });
        return bytes;
      }
    }

    return bytes;
  }

  async deserialize<T extends SnapshotBase>(data: Uint8Array): Promise<T> {
    let bytes = data;

    // Try to detect and decompress if needed
    if (data.length > 2 && data[0] === 0x1f && data[1] === 0x8b) {
      try {
        const { decompressBlob } = await import("@wf-agent/storage");
        bytes = await decompressBlob(data, "gzip");
      } catch (error) {
        logger.warn("Decompression failed, treating as uncompressed", { error });
      }
    }

    const json = new TextDecoder().decode(bytes);
    return JSON.parse(json) as T;
  }
}

/**
 * Plain JSON strategy (no compression)
 */
export class PlainJsonStrategy implements SerializationStrategy {
  readonly name = "plain-json";
  readonly supportsCompression = false;
  readonly supportsEncryption = false;
  readonly description = "Plain JSON serialization without compression";

  private readonly prettyPrint: boolean;

  constructor(options?: { prettyPrint?: boolean }) {
    this.prettyPrint = options?.prettyPrint ?? false;
  }

  async serialize<T extends SnapshotBase>(snapshot: T): Promise<Uint8Array> {
    const json = this.prettyPrint ? JSON.stringify(snapshot, null, 2) : JSON.stringify(snapshot);
    return new TextEncoder().encode(json);
  }

  async deserialize<T extends SnapshotBase>(data: Uint8Array): Promise<T> {
    const json = new TextDecoder().decode(data);
    return JSON.parse(json) as T;
  }
}

/**
 * Binary serialization strategy (using MessagePack or similar)
 * Note: This is a placeholder - actual implementation would require msgpack library
 */
export class MessagePackStrategy implements SerializationStrategy {
  readonly name = "msgpack";
  readonly supportsCompression = true;
  readonly supportsEncryption = false;
  readonly description = "Binary MessagePack serialization (requires msgpack library)";

  async serialize<T extends SnapshotBase>(_snapshot: T): Promise<Uint8Array> {
    throw new Error(
      "MessagePackStrategy requires msgpack library. Install @msgpack/msgpack and implement serialization.",
    );
  }

  async deserialize<T extends SnapshotBase>(_data: Uint8Array): Promise<T> {
    throw new Error(
      "MessagePackStrategy requires msgpack library. Install @msgpack/msgpack and implement deserialization.",
    );
  }
}

/**
 * Encrypted JSON strategy (placeholder for future implementation)
 */
export class EncryptedJsonStrategy implements SerializationStrategy {
  readonly name = "encrypted-json";
  readonly supportsCompression = true;
  readonly supportsEncryption = true;
  readonly description = "Encrypted JSON serialization (requires crypto implementation)";

  private readonly encryptionKey?: Uint8Array;

  constructor(options?: { encryptionKey?: Uint8Array }) {
    this.encryptionKey = options?.encryptionKey;
  }

  async serialize<T extends SnapshotBase>(_snapshot: T): Promise<Uint8Array> {
    throw new Error(
      "EncryptedJsonStrategy requires Web Crypto API implementation. Provide encryption key and implement encryption logic.",
    );
  }

  async deserialize<T extends SnapshotBase>(_data: Uint8Array): Promise<T> {
    throw new Error(
      "EncryptedJsonStrategy requires Web Crypto API implementation. Provide encryption key and implement decryption logic.",
    );
  }
}

/**
 * Strategy registry for managing multiple serialization strategies
 */
export class StrategyRegistry {
  private static instance: StrategyRegistry | null = null;
  private strategies: Map<string, SerializationStrategy> = new Map();
  private defaultStrategy: SerializationStrategy;
  private strategyByEntityType: Map<string, SerializationStrategy> = new Map();

  private constructor() {
    // Default to JSON+Gzip strategy
    this.defaultStrategy = new JsonGzipStrategy();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): StrategyRegistry {
    if (!StrategyRegistry.instance) {
      StrategyRegistry.instance = new StrategyRegistry();
    }
    return StrategyRegistry.instance;
  }

  /**
   * Reset singleton instance (useful for testing)
   */
  static resetInstance(): void {
    StrategyRegistry.instance = null;
  }

  /**
   * Register a strategy
   */
  registerStrategy(strategy: SerializationStrategy): void {
    this.strategies.set(strategy.name, strategy);
    logger.info(`Registered serialization strategy: ${strategy.name}`);
  }

  /**
   * Set default strategy
   */
  setDefaultStrategy(strategy: SerializationStrategy): void {
    this.defaultStrategy = strategy;
    logger.info(`Set default strategy to: ${strategy.name}`);
  }

  /**
   * Set strategy for specific entity type
   */
  setStrategyForType(entityType: string, strategyName: string): void {
    const strategy = this.strategies.get(strategyName);
    if (!strategy) {
      throw new Error(`Strategy not found: ${strategyName}`);
    }
    this.strategyByEntityType.set(entityType, strategy);
    logger.info(`Set strategy for entity type '${entityType}' to: ${strategyName}`);
  }

  /**
   * Get strategy for an entity type (or default)
   */
  getStrategy(entityType?: string): SerializationStrategy {
    if (entityType) {
      const strategy = this.strategyByEntityType.get(entityType);
      if (strategy) {
        return strategy;
      }
    }
    return this.defaultStrategy;
  }

  /**
   * Get all registered strategy names
   */
  getStrategyNames(): string[] {
    return Array.from(this.strategies.keys());
  }

  /**
   * Clear all strategies
   */
  clearStrategies(): void {
    this.strategies.clear();
    this.strategyByEntityType.clear();
    logger.info("Cleared all registered strategies");
  }
}

/**
 * Convenience function to get the strategy registry
 */
export function getStrategyRegistry(): StrategyRegistry {
  return StrategyRegistry.getInstance();
}
