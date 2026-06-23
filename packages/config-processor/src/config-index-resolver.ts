/**
 * Configuration Index Resolver Factory
 *
 * Provides a factory function to create index resolvers for each index type.
 * This bridges the SDK's registration pattern with config-processor's actual implementations.
 *
 * Usage:
 * ```ts
 * import { registerAllIndexResolvers } from "@wf-agent/config-processor";
 *
 * // Call during application initialization
 * registerAllIndexResolvers();
 *
 * // Now SDK's loadConfigIndex is ready to use
 * const index = await loadConfigIndex("workflows", "./configs/index.json");
 * ```
 */

import { registerResolver } from "@wf-agent/sdk/api";
import type { IndexType, IndexResolver } from "@wf-agent/sdk/api";
import {
  resolveLLMProfileIndex,
  resolveWorkflowIndex,
  resolveNodeTemplateIndex,
  resolveScriptIndex,
  resolvePromptTemplateIndex,
  resolveAgentLoopIndex,
} from "./config-index-loader.js";
import {
  resolveMcpPresetsIndex,
  resolveSkillPresetsIndex,
  resolveInfrastructurePresetsIndex,
} from "./preset-index-loaders.js";

/**
 * Create an index resolver for a specific index type
 *
 * @param type - The index type to create a resolver for
 * @returns The resolver function for that type
 * @throws Error if the index type is not supported
 */
export function createIndexResolver(type: IndexType): IndexResolver {
  const resolvers: Record<IndexType, IndexResolver> = {
    llm_profiles: resolveLLMProfileIndex,
    workflows: resolveWorkflowIndex,
    node_templates: resolveNodeTemplateIndex,
    scripts: resolveScriptIndex,
    prompt_templates: resolvePromptTemplateIndex,
    agent_loops: resolveAgentLoopIndex,
    mcp_presets: resolveMcpPresetsIndex,
    skill_presets: resolveSkillPresetsIndex,
    infrastructure_presets: resolveInfrastructurePresetsIndex,
  };

  const resolver = resolvers[type];
  if (!resolver) {
    throw new Error(`Unsupported index type: ${type}`);
  }

  return resolver;
}

/**
 * Register all configuration index resolvers to the SDK
 *
 * Must be called once during application initialization,
 * before using SDK's loadConfigIndex() function.
 *
 * This function registers resolvers for all supported index types:
 * - llm_profiles
 * - workflows
 * - node_templates
 * - scripts
 * - prompt_templates
 * - agent_loops
 * - mcp_presets
 * - skill_presets
 * - infrastructure_presets
 */
export function registerAllIndexResolvers(): void {
  const indexTypes: IndexType[] = [
    "llm_profiles",
    "workflows",
    "node_templates",
    "scripts",
    "prompt_templates",
    "agent_loops",
    "mcp_presets",
    "skill_presets",
    "infrastructure_presets",
  ];

  for (const type of indexTypes) {
    const resolver = createIndexResolver(type);
    registerResolver(type, resolver);
  }
}
