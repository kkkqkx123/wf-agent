/**
 * Agent Loop Checkpoint Configuration Parser
 *
 * Implements the specific configuration parsing logic for Agent Loop based on the sdk/shared/checkpoint common framework.
 */

import { CheckpointConfigResolver } from "../../../api/shared/config/processors/checkpoint-config.js";
import type {
  AgentLoopCheckpointConfig,
  CheckpointConfigResult,
  AgentLoopCheckpointConfigContext,
  AgentLoopCheckpointConfigLayer,
  CheckpointConfigSource,
  AgentCheckpointContentConfig,
} from "@wf-agent/types";

/**
 * Agent Loop Checkpoint Configuration Parser
 *
 * This extension enhances the general configuration parser by adding a configuration layer specific to Agent Loop.
 *
 * Configuration priority (from highest to lowest):
 * 1. runtime - Passed in during runtime
 * 2. agent - Agent Loop-specific configuration
 * 3. global - Global configuration
 * 4. default - Default values
 */
export class AgentLoopCheckpointConfigResolver extends CheckpointConfigResolver {
  /**
   * Parse Agent Loop checkpoint configuration
   *
   * @param layers List of configuration layers (sorted by priority from highest to lowest, with index 0 having the highest priority)
   * @param context Context of the checkpoint configuration
   * @returns Parsing result
   */
  resolveAgentConfig(
    layers: AgentLoopCheckpointConfigLayer[],
    context: AgentLoopCheckpointConfigContext,
  ): CheckpointConfigResult {
    // 1. Merge configurations (high priority overrides low priority)
    const mergedConfig = this.mergeConfigs(layers);

    // 2. Determine whether to enable it based on the timing of the trigger.
    const shouldCreate = this.evaluateTrigger(mergedConfig, context);

    // 3. Identify the source of the configuration that is actually taking effect.
    // Always find the effective source, regardless of whether checkpoint should be created
    const effectiveSource = this.findEffectiveSource(layers);

    return {
      shouldCreate,
      description: this.buildDescription(context),
      effectiveSource,
      triggerType: context.triggerType,
    };
  }

  /**
   * Merge the configuration layers
   * Higher priority layers (earlier in array) take precedence
   */
  private mergeConfigs(layers: AgentLoopCheckpointConfigLayer[]): AgentLoopCheckpointConfig {
    const result: AgentLoopCheckpointConfig = {};

    for (const layer of layers) {
      // Only set if not already defined (first/higher priority wins)
      if (layer.config.enabled !== undefined && result.enabled === undefined) {
        result.enabled = layer.config.enabled;
      }
      if (layer.config.interval !== undefined && result.interval === undefined) {
        result.interval = layer.config.interval;
      }
      if (layer.config.onErrorOnly !== undefined && result.onErrorOnly === undefined) {
        result.onErrorOnly = layer.config.onErrorOnly;
      }
      if (layer.config.deltaStorage !== undefined && result.deltaStorage === undefined) {
        result.deltaStorage = layer.config.deltaStorage;
      }
      if (layer.config.content !== undefined && result.content === undefined) {
        result.content = layer.config.content;
      }
    }

    return result;
  }

  /**
   * Evaluate whether to create a checkpoint based on the timing of the trigger.
   */
  private evaluateTrigger(
    config: AgentLoopCheckpointConfig,
    context: AgentLoopCheckpointConfigContext,
  ): boolean {
    // Disable globally
    if (config.enabled === false) return false;

    // Create only on error.
    if (config.onErrorOnly && !context.hasError) return false;

    // Check interval
    if (config.interval && config.interval > 1) {
      return context.currentIteration % config.interval === 0;
    }

    return true;
  }

  /**
   * Find the source of the configuration that is actually taking effect.
   */
  private findEffectiveSource(layers: AgentLoopCheckpointConfigLayer[]): CheckpointConfigSource {
    // Return the first configuration source that explicitly specifies "enabled".
    for (const layer of layers) {
      if (layer.config.enabled !== undefined) {
        return layer.source;
      }
    }
    return "default";
  }

  /**
   * Construct a checkpoint description
   */
  private buildDescription(context: AgentLoopCheckpointConfigContext): string {
    if (context.triggerType === "ERROR") {
      return "Error checkpoint";
    }
    return `Iteration ${context.currentIteration} checkpoint`;
  }
}

// Create a default parser instance
const defaultResolver = new AgentLoopCheckpointConfigResolver();

/**
 * Build Agent Loop checkpoint configuration layers
 *
 * Configuration priority (from highest to lowest):
 * 1. runtime - Passed in during runtime
 * 2. agent - Agent Loop-specific configuration
 * 3. global - Global configuration
 * 4. default - Default values
 *
 * @param globalConfig Global checkpoint configuration
 * @returns List of configuration layers (sorted by priority, highest first)
 */
export function buildAgentCheckpointLayers(
  globalConfig: AgentLoopCheckpointConfig | undefined,
): AgentLoopCheckpointConfigLayer[] {
  const layers: AgentLoopCheckpointConfigLayer[] = [];

  // 1. Global Configuration (Low Priority)
  if (globalConfig) {
    // Map AgentCheckpointPolicy triggers to config conditions
    const globalEnabled = globalConfig.enabled ?? true;

    if (globalEnabled !== undefined) {
      layers.push({
        source: "global",
        config: {
          enabled: globalEnabled,
          interval: globalConfig.interval,
          onErrorOnly: globalConfig.onErrorOnly,
          deltaStorage: globalConfig.deltaStorage,
          content: globalConfig.content,
        },
      });
    }
  }

  return layers;
}

/**
 * Parse checkpoint configuration
 *
 * Convenient function that uses the default parser.
 *
 * @param layers List of configuration layers
 * @param context Context for checkpoint configuration
 * @returns Parsing result
 */
export function resolveAgentCheckpointConfig(
  layers: AgentLoopCheckpointConfigLayer[],
  context: AgentLoopCheckpointConfigContext,
): CheckpointConfigResult {
  return defaultResolver.resolveAgentConfig(layers, context);
}

/**
 * Check whether it is necessary to create a checkpoint
 */
export function shouldCreateAgentCheckpoint(
  layers: AgentLoopCheckpointConfigLayer[],
  context: AgentLoopCheckpointConfigContext,
): boolean {
  return resolveAgentCheckpointConfig(layers, context).shouldCreate;
}

/**
 * Get the checkpoint description
 */
export function getAgentCheckpointDescription(
  layers: AgentLoopCheckpointConfigLayer[],
  context: AgentLoopCheckpointConfigContext,
): string | undefined {
  return resolveAgentCheckpointConfig(layers, context).description;
}

/**
 * Extract content config from resolved checkpoint config
 */
export function getAgentCheckpointContentConfig(
  layers: AgentLoopCheckpointConfigLayer[],
): AgentCheckpointContentConfig | undefined {
  for (const layer of layers) {
    if (layer.config.content !== undefined) {
      return layer.config.content;
    }
  }
  return undefined;
}
