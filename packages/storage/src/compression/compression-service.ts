/**
 * Unified Compression Service
 * Centralized compression configuration and management for all storage backends
 */

import { CompressionConfig, DEFAULT_COMPRESSION_CONFIG } from "../compression/compressor.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("compression-service");

/**
 * Global compression configuration
 */
export interface GlobalCompressionConfig {
  /** Default compression config for all entities */
  defaultConfig: CompressionConfig;
  /** Entity-specific compression configs */
  entityConfigs: {
    checkpoint?: CompressionConfig;
    workflow?: CompressionConfig;
    task?: CompressionConfig;
    execution?: CompressionConfig;
    agentLoop?: CompressionConfig;
    agentLoopCheckpoint?: CompressionConfig;
  };
}

/**
 * Default global compression configuration
 */
export const DEFAULT_GLOBAL_COMPRESSION_CONFIG: GlobalCompressionConfig = {
  defaultConfig: DEFAULT_COMPRESSION_CONFIG,
  entityConfigs: {
    checkpoint: {
      enabled: true,
      algorithm: "gzip",
      threshold: 1024, // 1KB
    },
    workflow: {
      enabled: true,
      algorithm: "brotli",
      threshold: 2048, // 2KB (workflows are typically larger)
    },
    task: {
      enabled: true,
      algorithm: "gzip",
      threshold: 512, // 512B (tasks are typically smaller)
    },
    execution: {
      enabled: true,
      algorithm: "gzip",
      threshold: 1024, // 1KB
    },
    agentLoop: {
      enabled: true,
      algorithm: "gzip",
      threshold: 1024, // 1KB
    },
    agentLoopCheckpoint: {
      enabled: true,
      algorithm: "gzip",
      threshold: 1024, // 1KB
    },
  },
};

/**
 * Entity type for compression configuration
 */
export type EntityType =
  | "checkpoint"
  | "workflow"
  | "task"
  | "execution"
  | "agentLoop"
  | "agentLoopCheckpoint";

/**
 * Compression Service
 * 
 * Singleton service that provides centralized compression configuration
 * for all storage backends. Allows runtime configuration updates.
 */
export class CompressionService {
  private static instance: CompressionService;
  private config: GlobalCompressionConfig;

  private constructor(config: GlobalCompressionConfig = DEFAULT_GLOBAL_COMPRESSION_CONFIG) {
    this.config = config;
    logger.debug("CompressionService initialized", {
      defaultAlgorithm: config.defaultConfig.algorithm,
      entityConfigs: Object.keys(config.entityConfigs),
    });
  }

  /**
   * Get singleton instance
   */
  static getInstance(): CompressionService {
    if (!CompressionService.instance) {
      CompressionService.instance = new CompressionService();
    }
    return CompressionService.instance;
  }

  /**
   * Reset singleton instance (useful for testing)
   */
  static resetInstance(): void {
    CompressionService.instance = new CompressionService();
  }

  /**
   * Configure compression service
   * @param config Partial configuration to merge with existing config
   */
  static configure(config: Partial<GlobalCompressionConfig>): void {
    const instance = CompressionService.getInstance();
    instance.config = {
      ...instance.config,
      ...config,
      entityConfigs: {
        ...instance.config.entityConfigs,
        ...config.entityConfigs,
      },
    };
    logger.info("CompressionService configured", {
      updatedEntityConfigs: config.entityConfigs ? Object.keys(config.entityConfigs) : [],
    });
  }

  /**
   * Get compression config for a specific entity type
   * @param entityType Optional entity type (checkpoint, workflow, task, etc.)
   * @returns Compression config for the entity or default config
   */
  getConfig(entityType?: EntityType): CompressionConfig {
    if (entityType && this.config.entityConfigs[entityType]) {
      const entityConfig = this.config.entityConfigs[entityType]!;
      logger.debug("Getting entity-specific compression config", {
        entityType,
        algorithm: entityConfig.algorithm,
        threshold: entityConfig.threshold,
      });
      return entityConfig;
    }
    
    logger.debug("Using default compression config", {
      algorithm: this.config.defaultConfig.algorithm,
      threshold: this.config.defaultConfig.threshold,
    });
    return this.config.defaultConfig;
  }

  /**
   * Get current global configuration
   */
  getGlobalConfig(): GlobalCompressionConfig {
    return { ...this.config };
  }

  /**
   * Check if compression is enabled for an entity type
   */
  isEnabled(entityType?: EntityType): boolean {
    const config = this.getConfig(entityType);
    return config.enabled;
  }
}
