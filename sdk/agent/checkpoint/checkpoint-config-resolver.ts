/**
 * Agent Loop Checkpoint Configuration Parser
 *
 * Implements the specific configuration parsing logic for Agent Loop based on the sdk/core/checkpoint common framework.
 */

import { CheckpointConfigResolver } from "../../core/utils/checkpoint/checkpoint-config-resolver.js";
import type {
  AgentLoopCheckpointConfig,
  CheckpointConfigResult,
  AgentLoopCheckpointConfigContext,
  AgentLoopCheckpointConfigLayer,
  CheckpointConfigSource,
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
export class AgentLoopCheckpointResolver extends CheckpointConfigResolver {
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
    const effectiveSource = shouldCreate ? this.findEffectiveSource(layers) : "default";

    return {
      shouldCreate,
      description: this.buildDescription(context),
      effectiveSource,
      triggerType: context.triggerType,
    };
  }

  /**
   * Merge the configuration layers
   */
  private mergeConfigs(layers: AgentLoopCheckpointConfigLayer[]): AgentLoopCheckpointConfig {
    const result: AgentLoopCheckpointConfig = {};

    for (const layer of layers) {
      if (layer.config.enabled !== undefined) {
        result.enabled = layer.config.enabled;
      }
      if (layer.config.interval !== undefined) {
        result.interval = layer.config.interval;
      }
      if (layer.config.onErrorOnly !== undefined) {
        result.onErrorOnly = layer.config.onErrorOnly;
      }
      if (layer.config.deltaStorage !== undefined) {
        result.deltaStorage = layer.config.deltaStorage;
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
