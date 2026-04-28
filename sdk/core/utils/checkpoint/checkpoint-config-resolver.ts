/**
 * Universal Checkpoint Configuration Parser
 *
 * Provides priority-based parsing logic for checkpoint configurations.
 * Supports multi-level configurations, with the specific levels defined by subclasses.
 */

import type { CheckpointConfigSource, CheckpointConfigResult } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CheckpointConfigResolver" });

/**
 * Configuration hierarchy definition
 */
export interface ConfigLayer {
  /** Level names */
  name: string;
  /** Whether to enable checkpoints */
  enabled?: boolean;
  /** Checkpoint description */
  description?: string;
}

/**
 * Configure parser options
 */
export interface ConfigResolverOptions {
  /** Is the default setting enabled? */
  defaultEnabled?: boolean;
  /** Default description */
  defaultDescription?: string;
}

/**
 * General Configuration Parser Base Class
 *
 * Provides the general logic for parsing configuration priorities.
 * Subclasses can be extended to add configuration layers for specific scenarios.
 */
export abstract class CheckpointConfigResolver {
  protected defaultEnabled: boolean;
  protected defaultDescription: string;

  constructor(options: ConfigResolverOptions = {}) {
    this.defaultEnabled = options.defaultEnabled ?? false;
    this.defaultDescription = options.defaultDescription ?? "Checkpoint";
  }

  /**
   * Parse Configuration
   *
   * Check each level of configuration in descending order of priority and return the result of the first configuration that is clearly defined.
   * The caller should pass in the layers in the order of priority (index 0 has the highest priority).
   *
   * @param layers List of configuration levels, sorted in descending order of priority
   * @returns The result of the parsing
   */
  resolve(layers: ConfigLayer[]): CheckpointConfigResult {
    // Traverse all levels to find the first one that is explicitly configured.
    for (const layer of layers) {
      if (layer.enabled !== undefined) {
        logger.debug("Checkpoint config resolved", {
          layer: layer.name,
          enabled: layer.enabled,
          description: layer.description || this.defaultDescription,
        });
        return {
          shouldCreate: layer.enabled,
          description: layer.description || this.defaultDescription,
          effectiveSource: layer.name as CheckpointConfigSource,
        };
      }
    }

    // No explicit configuration is provided; default values are used.
    logger.debug("Checkpoint config using default", {
      enabled: this.defaultEnabled,
      description: this.defaultDescription,
    });
    return {
      shouldCreate: this.defaultEnabled,
      description: this.defaultDescription,
      effectiveSource: "default",
    };
  }

  /**
   * Create a configuration level
   *
   * Factory method used to create a configuration level object.
   *
   * @param name: The name of the level
   * @param config: The configuration content
   * @returns: The configuration level
   */
  protected createLayer(
    name: string,
    config?: {
      enabled?: boolean;
      description?: string;
    },
  ): ConfigLayer {
    return {
      name,
      ...config,
    };
  }
}

/**
 * Check if a checkpoint should be created
 *
 * A convenient function for making a quick decision.
 *
 * @param resolver: The configuration resolver
 * @param layers: The configuration layers
 * @returns: Whether to create the checkpoint
 */
export function shouldCreateCheckpoint(
  resolver: CheckpointConfigResolver,
  layers: ConfigLayer[],
): boolean {
  return resolver.resolve(layers).shouldCreate;
}

/**
 * Get checkpoint description
 *
 * A convenient function for retrieving the description.
 *
 * @param resolver: The configuration resolver
 * @param layers: The configuration layers
 * @returns: The description
 */
export function getCheckpointDescription(
  resolver: CheckpointConfigResolver,
  layers: ConfigLayer[],
): string | undefined {
  return resolver.resolve(layers).description;
}
