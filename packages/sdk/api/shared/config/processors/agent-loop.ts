/**
 * Agent Loop Configuration Processor
 *
 * Handles the parsing and transformation of AgentLoopConfigFile to AgentLoopRuntimeConfig
 */

import type { AgentLoopRuntimeConfig, AgentHook, AgentHookType } from "@wf-agent/types";
import type { Condition } from "@wf-agent/types";
import type { AgentLoopConfigFile, AgentHookConfigFile } from "../types.js";

/**
 * Convert the configuration file format to runtime configuration
 * @param configFile The agent loop configuration file
 * @returns AgentLoopRuntimeConfig
 */
export function transformToAgentLoopConfig(
  configFile: AgentLoopConfigFile,
): AgentLoopRuntimeConfig {
  const config: AgentLoopRuntimeConfig = {
    profileId: configFile.profileId,
    systemPrompt: configFile.systemPrompt,
    systemPromptTemplateId: configFile.systemPromptTemplateId,
    systemPromptTemplateVariables: configFile.systemPromptTemplateVariables,
    maxIterations: configFile.maxIterations,
    initialMessages: configFile.initialMessages,
    stream: configFile.stream,
    createCheckpointOnEnd: configFile.checkpoint?.createOnEnd,
    createCheckpointOnError: configFile.checkpoint?.createOnError,
  };

  // Handle availableTools configuration
  if (configFile.availableTools) {
    config.availableTools = configFile.availableTools;
  }

  if (configFile.hooks && configFile.hooks.length > 0) {
    config.hooks = configFile.hooks.map(hook => transformHook(hook));
  }

  return config;
}

/**
 * Export Agent Loop configuration
 * Returns typed data ready for serialization.
 * @param configFile AgentLoopConfigFile object
 * @returns The agent loop config ready for export
 */
export function exportAgentLoopConfig(configFile: AgentLoopConfigFile): AgentLoopConfigFile {
  return configFile;
}

/**
 * Translate Hook Configuration
 * @param hookFile The Hook configuration file
 * @returns AgentHook
 */
function transformHook(hookFile: AgentHookConfigFile): AgentHook {
  const hook: AgentHook = {
    hookType: hookFile.hookType as AgentHookType,
    eventName: hookFile.eventName,
    enabled: hookFile.enabled,
    weight: hookFile.weight,
    eventPayload: hookFile.eventPayload,
    createCheckpoint: hookFile.createCheckpoint,
    checkpointDescription: hookFile.checkpointDescription,
  };

  // Parse conditional expressions
  if (hookFile.condition) {
    hook.condition = parseCondition(hookFile.condition);
  }

  return hook;
}

/**
 * Parse a conditional expression
 * @param conditionStr: The condition string
 * @returns: A Condition object
 */
function parseCondition(conditionStr: string): Condition {
  // Returns expression conditions with explicit type
  return {
    type: "expression",
    expression: conditionStr,
  } as Condition;
}
