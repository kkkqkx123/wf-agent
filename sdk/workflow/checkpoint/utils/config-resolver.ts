/**
 * Workflow Checkpoint Configuration Parser
 *
 * Implements Workflow-specific configuration parsing logic based on the sdk/core/checkpoint generic framework.
 * Handles multi-level configuration priority rules.
 */

import type {
  CheckpointConfig,
  Node,
  CheckpointConfigContext,
  CheckpointConfigResult,
  GraphCheckpointConfigLayer,
  CheckpointConfigContent,
  CheckpointConfigSource,
  GraphCheckpointTriggerType,
} from "@wf-agent/types";
import { CheckpointConfigResolver } from "../../../core/utils/checkpoint/checkpoint-config-resolver.js";

/**
 * Workflow Checkpoint Configuration Parser
 *
 * This extension expands the general configuration parser to include Workflow-specific configuration levels.
 *
 * Configuration priority (from highest to lowest):
 * 1. runtime - Passed in during runtime
 * 2. workflow - Defined by the workflow
 * 3. node - Defined by the node
 * 4. global - Global configuration
 * 5. default - Default values
 */
export class WorkflowCheckpointConfigResolver extends CheckpointConfigResolver {
  /**
   * Parse Workflow checkpoint configuration
   *
   * @param layers List of configuration layers (sorted by priority from highest to lowest, with index 0 having the highest priority)
   * @param context Context of the checkpoint configuration
   * @returns Parsing result
   */
  resolveWorkflowConfig(
    layers: GraphCheckpointConfigLayer[],
    context: CheckpointConfigContext,
  ): CheckpointConfigResult {
    // Special Handling: Triggered sub-workflows do not create checkpoints by default.
    if (context.isTriggeredSubworkflow && !context.explicitEnableCheckpoint) {
      return {
        shouldCreate: false,
        effectiveSource: "default",
        triggerType: context.triggerType,
      };
    }

    // 1. Merge configurations
    const mergedConfig = this.mergeConfigs(layers);

    // 2. Evaluate based on the timing of the trigger.
    const shouldCreate = this.evaluateTriggerForWorkflow(mergedConfig, context);

    // 3. Locate the source of the configuration that is actually taking effect.
    const effectiveSource = this.findEffectiveSource(layers);

    return {
      shouldCreate,
      description: this.buildDescription(context, mergedConfig),
      effectiveSource,
      triggerType: context.triggerType,
    };
  }

  /**
   * Merge the configuration layers
   */
  private mergeConfigs(layers: GraphCheckpointConfigLayer[]): CheckpointConfigContent {
    const result: CheckpointConfigContent = {};

    for (const layer of layers) {
      if (layer.config.enabled !== undefined) {
        result.enabled = layer.config.enabled;
      }
      if (layer.config.description !== undefined) {
        result.description = layer.config.description;
      }
      if (layer.config.triggers !== undefined) {
        result.triggers = {
          ...result.triggers,
          ...layer.config.triggers,
        };
      }
    }

    return result;
  }

  /**
   * Evaluate based on the timing of the trigger (Workflow scenario)
   */
  private evaluateTriggerForWorkflow(
    config: CheckpointConfigContent,
    context: CheckpointConfigContext,
  ): boolean {
    if (config.enabled === false) return false;

    // Check the corresponding enablement configuration based on the triggering timing.
    const triggerConfig = config.triggers || {};

    switch (context.triggerType) {
      case "NODE_BEFORE_EXECUTE":
        return triggerConfig.nodeBeforeExecute !== false;
      case "NODE_AFTER_EXECUTE":
        return triggerConfig.nodeAfterExecute !== false;
      case "TOOL_BEFORE":
        return triggerConfig.toolBefore !== false;
      case "TOOL_AFTER":
        return triggerConfig.toolAfter !== false;
      case "HOOK":
      case "TRIGGER":
        return true; // Hook and Trigger are enabled by default.
      default:
        return false;
    }
  }

  /**
   * Find the source of the configuration that is actually taking effect.
   */
  private findEffectiveSource(layers: GraphCheckpointConfigLayer[]): CheckpointConfigSource {
    // Return the first configuration source that explicitly specifies 'enabled'.
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
  private buildDescription(
    context: CheckpointConfigContext,
    config: CheckpointConfigContent,
  ): string {
    if (config.description) {
      return config.description;
    }

    const triggerDesc: Record<GraphCheckpointTriggerType, string> = {
      NODE_BEFORE_EXECUTE: "Before node",
      NODE_AFTER_EXECUTE: "After node",
      TOOL_BEFORE: "Before tool",
      TOOL_AFTER: "After tool",
      HOOK: "Hook",
      TRIGGER: "Trigger",
    };

    return `${triggerDesc[context.triggerType]} checkpoint`;
  }
}

// Create a default parser instance
const defaultResolver = new WorkflowCheckpointConfigResolver();

/**
 * Constructing the node checkpoint configuration layer
 *
 * @param globalConfig: Global checkpoint configuration
 * @param node: Node configuration
 * @param context: Checkpoint configuration context
 * @returns: List of configuration levels (sorted in descending order of priority)
 */
export function buildNodeCheckpointLayers(
  globalConfig: CheckpointConfig | undefined,
  node: Node | undefined,
  context: CheckpointConfigContext,
): GraphCheckpointConfigLayer[] {
  const layers: GraphCheckpointConfigLayer[] = [];

  // 1. Node configuration (high priority)
  if (node) {
    const nodeEnabled =
      context.triggerType === "NODE_BEFORE_EXECUTE"
        ? node.checkpointBeforeExecute
        : node.checkpointAfterExecute;

    if (nodeEnabled !== undefined) {
      layers.push({
        source: "node",
        config: {
          enabled: nodeEnabled,
          description: `${context.triggerType === "NODE_BEFORE_EXECUTE" ? "Before" : "After"} node: ${node.name}`,
        },
      });
    }
  }

  // 2. Global Configuration (Low Priority)
  if (globalConfig) {
    const globalEnabled =
      context.triggerType === "NODE_BEFORE_EXECUTE"
        ? globalConfig.checkpointBeforeNode
        : globalConfig.checkpointAfterNode;

    if (globalEnabled !== undefined) {
      layers.push({
        source: "global",
        config: {
          enabled: globalEnabled,
          description: `Global checkpoint ${context.triggerType === "NODE_BEFORE_EXECUTE" ? "before" : "after"} node`,
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
export function resolveCheckpointConfig(
  layers: GraphCheckpointConfigLayer[],
  context: CheckpointConfigContext,
): CheckpointConfigResult {
  return defaultResolver.resolveWorkflowConfig(layers, context);
}

/**
 * Check whether it is necessary to create a checkpoint.
 */
export function shouldCreateCheckpoint(
  layers: GraphCheckpointConfigLayer[],
  context: CheckpointConfigContext,
): boolean {
  return resolveCheckpointConfig(layers, context).shouldCreate;
}

/**
 * Get the checkpoint description.
 */
export function getCheckpointDescription(
  layers: GraphCheckpointConfigLayer[],
  context: CheckpointConfigContext,
): string | undefined {
  return resolveCheckpointConfig(layers, context).description;
}
