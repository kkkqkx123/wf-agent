/**
 * Metadata Injection Utilities
 *
 * Unified functions for injecting skill, workflow, and agent metadata
 * into agent loop runtime configurations.
 *
 * Design Principles:
 * 1. Conditional injection - only inject metadata when corresponding tool is in availableTools
 * 2. Centralized logic - single source of truth for all metadata injection
 * 3. Type-safe - works with both AgentToolConfig and AvailableTools
 */

import type { AgentToolConfig, SkillMetadata } from "@wf-agent/types";
import type { AvailableTools } from "@wf-agent/types";
import type { SkillRegistry } from "../registry/skill-registry.js";
import type { GlobalContext } from "../global-context.js";
import * as Identifiers from "../di/service-identifiers.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetadataInjection" });

// ---------------------------------------------------------------------------
// Tool Names
// ---------------------------------------------------------------------------

/**
 * Reserved tool names for metadata injection
 */
export const METADATA_TOOL_NAMES = {
  /** Skill tool - for progressive disclosure of skill content */
  SKILL: "skill",
  /** Workflow execution tool - for running subworkflows */
  EXECUTE_WORKFLOW: "execute_workflow",
  /** Agent call tool - for invoking other agents */
  CALL_AGENT: "call_agent",
} as const;

// ---------------------------------------------------------------------------
// Tool Availability Checkers
// ---------------------------------------------------------------------------

/**
 * Check if a specific tool is available in AgentToolConfig
 */
export function isToolInAgentConfig(
  config: AgentToolConfig | undefined,
  toolName: string,
): boolean {
  return config?.tools?.includes(toolName) ?? false;
}

/**
 * Check if a specific tool is available in AvailableTools (workflow)
 */
export function isToolInWorkflowConfig(
  config: AvailableTools | undefined,
  toolName: string,
): boolean {
  if (!config) return false;
  // Check both 'available' (schema tools) and 'initial' (enabled at startup)
  const availableTools = config.available || [];
  const initialTools = config.initial || [];
  return availableTools.includes(toolName) || initialTools.includes(toolName);
}

/**
 * Unified tool availability check for any config type
 */
export function isToolAvailable(
  config: AgentToolConfig | AvailableTools | undefined,
  toolName: string,
): boolean {
  if (!config) return false;

  // AgentToolConfig has 'tools' array
  if ("tools" in config) {
    return isToolInAgentConfig(config as AgentToolConfig, toolName);
  }

  // AvailableTools has 'available' array
  if ("available" in config) {
    return isToolInWorkflowConfig(config as AvailableTools, toolName);
  }

  return false;
}

// ---------------------------------------------------------------------------
// Tool List Mutators
// ---------------------------------------------------------------------------

/**
 * Ensure a tool is in the AgentToolConfig tools list
 * Returns a new config object (does not mutate input)
 */
export function ensureToolInAgentConfig(
  config: AgentToolConfig | undefined,
  toolName: string,
): AgentToolConfig {
  if (!config) {
    return { tools: [toolName] };
  }

  if (config.tools.includes(toolName)) {
    return config;
  }

  return {
    ...config,
    tools: [...config.tools, toolName],
  };
}

/**
 * Ensure a tool is in the AvailableTools available list
 * Returns a new config object (does not mutate input)
 */
export function ensureToolInWorkflowConfig(
  config: AvailableTools | undefined,
  toolName: string,
): AvailableTools {
  if (!config) {
    return { available: [toolName] };
  }

  if (config.available.includes(toolName)) {
    return config;
  }

  return {
    ...config,
    available: [...config.available, toolName],
  };
}

// ---------------------------------------------------------------------------
// Skill Metadata Injection
// ---------------------------------------------------------------------------

/**
 * Options for skill metadata injection
 */
export interface SkillInjectionOptions {
  /** The runtime config's availableTools */
  availableTools?: AgentToolConfig | AvailableTools;
  /** Current system prompt */
  systemPrompt: string;
  /** Whether to auto-add skill tool if skills exist but tool is missing */
  autoAddTool?: boolean;
}

/**
 * Result of skill metadata injection
 */
export interface SkillInjectionResult {
  /** Updated system prompt (with skill metadata injected) */
  systemPrompt: string;
  /** Updated availableTools (with skill tool added if autoAddTool was true) */
  availableTools?: AgentToolConfig | AvailableTools;
  /** Whether skill metadata was actually injected */
  injected: boolean;
  /** Number of enabled skills */
  skillCount: number;
}

/**
 * Inject skill metadata into system prompt if conditions are met.
 *
 * Conditions for injection:
 * 1. SkillRegistry is available and has enabled skills
 * 2. 'skill' tool is in availableTools (or autoAddTool is true)
 *
 * @param skillRegistry - The skill registry instance
 * @param options - Injection options
 * @returns Injection result with updated prompt and config
 */
export function injectSkillMetadata(
  skillRegistry: SkillRegistry | undefined,
  options: SkillInjectionOptions,
): SkillInjectionResult {
  const { availableTools, systemPrompt, autoAddTool = true } = options;

  // Check if skill registry exists and has enabled skills
  if (!skillRegistry) {
    logger.debug("SkillRegistry not available, skipping skill metadata injection");
    return { systemPrompt, availableTools, injected: false, skillCount: 0 };
  }

  const enabledSkills = skillRegistry.getEnabledSkills();
  if (enabledSkills.length === 0) {
    logger.debug("No enabled skills, skipping skill metadata injection");
    return { systemPrompt, availableTools, injected: false, skillCount: 0 };
  }

  // Check if 'skill' tool is available
  const hasSkillTool = isToolAvailable(availableTools, METADATA_TOOL_NAMES.SKILL);

  let updatedTools = availableTools;

  if (!hasSkillTool && autoAddTool) {
    // Auto-add skill tool to the config
    logger.debug("Auto-adding 'skill' tool to availableTools");
    if (!availableTools) {
      updatedTools = { tools: [METADATA_TOOL_NAMES.SKILL] };
    } else if ("tools" in availableTools) {
      updatedTools = ensureToolInAgentConfig(availableTools, METADATA_TOOL_NAMES.SKILL);
    } else if ("available" in availableTools) {
      updatedTools = ensureToolInWorkflowConfig(availableTools, METADATA_TOOL_NAMES.SKILL);
    }
  } else if (!hasSkillTool) {
    // Tool not available and auto-add disabled
    logger.debug("'skill' tool not in availableTools and autoAddTool is false, skipping injection");
    return { systemPrompt, availableTools, injected: false, skillCount: enabledSkills.length };
  }

  // Inject skill metadata into system prompt
  const updatedPrompt = skillRegistry.injectSkillMetadata(systemPrompt);

  logger.debug("Skill metadata injected", {
    skillCount: enabledSkills.length,
    skills: enabledSkills.map((s: SkillMetadata) => s.name).join(", "),
  });

  return {
    systemPrompt: updatedPrompt,
    availableTools: updatedTools,
    injected: true,
    skillCount: enabledSkills.length,
  };
}

// ---------------------------------------------------------------------------
// Unified Metadata Injection
// ---------------------------------------------------------------------------

/**
 * Options for unified metadata injection
 */
export interface MetadataInjectionOptions {
  /** Current system prompt */
  systemPrompt: string;
  /** Current availableTools config */
  availableTools?: AgentToolConfig | AvailableTools;
  /** Whether to auto-add tools when metadata exists but tool is missing */
  autoAddTools?: boolean;
}

/**
 * Result of unified metadata injection
 */
export interface MetadataInjectionResult {
  /** Updated system prompt */
  systemPrompt: string;
  /** Updated availableTools */
  availableTools?: AgentToolConfig | AvailableTools;
  /** Injection summary */
  summary: {
    skillsInjected: boolean;
    skillCount: number;
    workflowsInjected: boolean;
    agentsInjected: boolean;
  };
}

/**
 * Inject all applicable metadata (skills, workflows, agents) into the config.
 *
 * This is the main entry point for metadata injection in agent loop execution.
 * Each metadata type is only injected if the corresponding tool is available.
 *
 * @param globalContext - The global context with DI container
 * @param options - Injection options
 * @returns Updated config with all metadata injected
 */
export function injectAllMetadata(
  globalContext: GlobalContext,
  options: MetadataInjectionOptions,
): MetadataInjectionResult {
  const { systemPrompt, availableTools, autoAddTools = true } = options;

  let currentPrompt = systemPrompt;
  let currentTools = availableTools;

  const summary = {
    skillsInjected: false,
    skillCount: 0,
    workflowsInjected: false,
    agentsInjected: false,
  };

  // 1. Inject skill metadata
  try {
    const skillRegistry = globalContext.container.get(Identifiers.SkillRegistry) as
      | SkillRegistry
      | undefined;

    const skillResult = injectSkillMetadata(skillRegistry, {
      systemPrompt: currentPrompt,
      availableTools: currentTools,
      autoAddTool: autoAddTools,
    });

    currentPrompt = skillResult.systemPrompt;
    currentTools = skillResult.availableTools;
    summary.skillsInjected = skillResult.injected;
    summary.skillCount = skillResult.skillCount;
  } catch (error) {
    logger.debug("Failed to inject skill metadata", { error });
  }

  // 2. Inject workflow metadata (if execute_workflow tool is available)
  // TODO: Implement workflow metadata injection when WorkflowRegistry is available
  // For now, this is handled separately in agent-loop-adapter.ts

  // 3. Inject agent metadata (if call_agent tool is available)
  // TODO: Implement agent metadata injection when AgentRegistry is available

  return {
    systemPrompt: currentPrompt,
    availableTools: currentTools,
    summary,
  };
}

// ---------------------------------------------------------------------------
// Convenience Functions for Runtime Config
// ---------------------------------------------------------------------------

/**
 * Agent loop runtime config shape (partial, for type safety)
 */
interface AgentLoopRuntimeConfigLike {
  systemPrompt?: string;
  availableTools?: AgentToolConfig;
}

/**
 * Apply skill metadata injection to an agent loop runtime config.
 * Mutates the config object in place for convenience.
 *
 * @param config - The runtime config to modify
 * @param globalContext - The global context with DI container
 * @returns True if any metadata was injected
 */
export function applyMetadataToConfig(
  config: AgentLoopRuntimeConfigLike,
  globalContext: GlobalContext,
): boolean {
  const result = injectAllMetadata(globalContext, {
    systemPrompt: config.systemPrompt || "",
    availableTools: config.availableTools,
    autoAddTools: true,
  });

  config.systemPrompt = result.systemPrompt;
  config.availableTools = result.availableTools as AgentToolConfig | undefined;

  return (
    result.summary.skillsInjected ||
    result.summary.workflowsInjected ||
    result.summary.agentsInjected
  );
}
