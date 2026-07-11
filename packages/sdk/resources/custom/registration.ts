/**
 * Custom Resources Registration
 *
 * Registers custom resources (tools, triggers, prompts) that were loaded from configuration files.
 * Implements the same registration pattern as predefined resources with appropriate error handling.
 *
 * Key features:
 * - Prevents overwriting predefined resources (name collision detection)
 * - Aggregates success and failure results
 * - Uses the same registry interfaces as predefined resources
 */

import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { PromptTemplateRegistry } from "@sdk/shared/registry/prompt-template-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { toSdkTool } from "@sdk/services/tools/utils.js";
import type {
  CustomToolDefinition,
  CustomTriggerDefinition,
  CustomPromptDefinition,
  CustomResources,
} from "./types.js";
import type { PromptTemplate, PromptVariableDefinition } from "@wf-agent/types";

const logger = createContextualLogger({ component: "CustomResourcesRegistration" });

/**
 * Result type for custom resources registration
 */
export interface CustomResourcesRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
  triggers: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
  prompts: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
}

/**
 * Register custom tools from loaded definitions
 *
 * Converts custom tool definitions to SDK format and registers them.
 * Prevents registration if a tool with the same ID already exists
 * (to avoid overwriting predefined resources).
 *
 * @param toolRegistry Tool registry to register into
 * @param tools Array of custom tool definitions
 * @returns Registration result with successes and failures
 */
export function registerCustomTools(
  toolRegistry: ToolRegistry,
  tools: CustomToolDefinition[],
): {
  success: string[];
  failures: Array<{ id: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const toolDef of tools) {
    try {
      // Check for collision with existing resources (including predefined)
      if (toolRegistry.has(toolDef.id)) {
        const errorMsg = `Tool '${toolDef.id}' already exists (may be predefined). Custom resources cannot override existing tools.`;
        failures.push({ id: toolDef.id, error: errorMsg });
        logger.warn(errorMsg);
        continue;
      }

      // Convert to SDK tool format and register
      // ToolDefinitionLike expects: id, description, parameters (not schema), type, etc.
      const sdkTool = toSdkTool({
        id: toolDef.id,
        type: toolDef.type,
        description: toolDef.description.summary,
        parameters: toolDef.schema,
        metadata: toolDef.metadata,
        // Note: handler, config loading would be implemented in Phase 3.2
        // For now, custom tools without execute/factory/config cannot be executed
      });

      toolRegistry.register(sdkTool);
      success.push(toolDef.id);
      logger.info(`Registered custom tool: ${toolDef.id}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: toolDef.id, error: errorMsg });
      logger.error(`Failed to register custom tool: ${toolDef.id}`, { error: errorMsg });
    }
  }

  return { success, failures };
}

/**
 * Register custom triggers from loaded definitions
 *
 * Registers custom triggers into the trigger template registry.
 * Prevents registration if a trigger with the same name already exists
 * (to avoid overwriting predefined resources).
 *
 * @param triggerRegistry Trigger registry to register into
 * @param triggers Array of custom trigger definitions
 * @returns Registration result with successes and failures
 */
export function registerCustomTriggers(
  triggerRegistry: TriggerTemplateRegistry,
  triggers: CustomTriggerDefinition[],
): {
  success: string[];
  failures: Array<{ id: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const triggerDef of triggers) {
    try {
      // Check for collision with existing resources (including predefined)
      if (triggerRegistry.has(triggerDef.name)) {
        const errorMsg = `Trigger '${triggerDef.name}' already exists (may be predefined). Custom resources cannot override existing triggers.`;
        failures.push({ id: triggerDef.name, error: errorMsg });
        logger.warn(errorMsg);
        continue;
      }

      // Get current time for trigger template (as Unix timestamp)
      const now = Date.now();

      // Construct a complete TriggerTemplate from the custom definition
      // Note: Custom trigger definitions have simplified condition/action structures
      // Real implementations should extend CustomTriggerDefinition with full trigger structure
      // For now, we provide a default send_notification action with required message field
      const triggerTemplate = {
        name: triggerDef.name,
        description: triggerDef.description,
        condition: {
          eventType: "CUSTOM_EVENT" as any,
          eventName: triggerDef.condition.value,
        },
        action: {
          type: "send_notification" as const,
          parameters: {
            message: `Trigger: ${triggerDef.name}`,
          },
        },
        createdAt: now,
        updatedAt: now,
        metadata: {
          ...triggerDef.metadata,
          source: "custom",
          conditionType: triggerDef.condition.type,
          conditionValue: triggerDef.condition.value,
        },
      };

      // Register trigger template
      triggerRegistry.register(triggerTemplate);

      success.push(triggerDef.name);
      logger.info(`Registered custom trigger: ${triggerDef.name}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: triggerDef.name, error: errorMsg });
      logger.error(`Failed to register custom trigger: ${triggerDef.name}`, { error: errorMsg });
    }
  }

  return { success, failures };
}

/**
 * Register custom prompts from loaded definitions
 *
 * Registers custom prompts into a prompt registry.
 * Converts custom prompt definitions to PromptTemplate format and registers them.
 * Prevents registration if a prompt with the same ID already exists.
 *
 * @param promptRegistry Prompt template registry to register into
 * @param prompts Array of custom prompt definitions
 * @returns Registration result with successes and failures
 */
export function registerCustomPrompts(
  promptRegistry: PromptTemplateRegistry,
  prompts: CustomPromptDefinition[],
): {
  success: string[];
  failures: Array<{ id: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const promptDef of prompts) {
    try {
      // Check for collision with existing resources
      if (promptRegistry.has(promptDef.id)) {
        const errorMsg = `Prompt '${promptDef.id}' already exists. Custom resources cannot override existing prompts.`;
        failures.push({ id: promptDef.id, error: errorMsg });
        logger.warn(errorMsg);
        continue;
      }

      // Map custom prompt type to PromptTemplate category
      const categoryMap: Record<string, PromptTemplate["category"]> = {
        system: "system",
        user: "user-command",
        assistant: "dynamic",
      };
      const category = categoryMap[promptDef.type] || "dynamic";

      // Convert string variables to PromptVariableDefinition[]
      const variables: PromptVariableDefinition[] | undefined = promptDef.variables?.map(v => ({
        name: v,
        type: "string" as const,
        required: false,
        description: `Variable: ${v}`,
      }));

      // Convert to PromptTemplate format
      const template: PromptTemplate = {
        id: promptDef.id,
        name: promptDef.name,
        description: (promptDef.metadata?.["description"] as string) || promptDef.name,
        category,
        content: promptDef.content,
        variables: variables && variables.length > 0 ? variables : undefined,
      };

      promptRegistry.register(template.id, template);
      success.push(promptDef.id);
      logger.info(`Registered custom prompt: ${promptDef.id}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: promptDef.id, error: errorMsg });
      logger.error(`Failed to register custom prompt: ${promptDef.id}`, { error: errorMsg });
    }
  }

  return { success, failures };
}

/**
 * Register all custom resources
 *
 * Coordinates registration of tools, triggers, and prompts.
 * Aggregates results from all three registration pipelines.
 *
 * @param registries Object containing toolRegistry, triggerRegistry, and promptRegistry
 * @param customResources Loaded custom resources to register
 * @returns Aggregated registration results
 */
export function registerCustomResources(
  registries: {
    toolRegistry: ToolRegistry;
    triggerRegistry: TriggerTemplateRegistry;
    promptRegistry: PromptTemplateRegistry;
  },
  customResources: CustomResources,
): CustomResourcesRegistrationResult {
  const results: CustomResourcesRegistrationResult = {
    tools: { success: [], failures: [] },
    triggers: { success: [], failures: [] },
    prompts: { success: [], failures: [] },
  };

  // Register tools
  if (customResources.tools.length > 0) {
    try {
      results.tools = registerCustomTools(registries.toolRegistry, customResources.tools);
      logger.info(`Custom tools registered: ${results.tools.success.length} succeeded`);
    } catch (error) {
      logger.error("Failed to register custom tools", { error });
    }
  }

  // Register triggers
  if (customResources.triggers.length > 0) {
    try {
      results.triggers = registerCustomTriggers(
        registries.triggerRegistry,
        customResources.triggers,
      );
      logger.info(`Custom triggers registered: ${results.triggers.success.length} succeeded`);
    } catch (error) {
      logger.error("Failed to register custom triggers", { error });
    }
  }

  // Register prompts
  if (customResources.prompts.length > 0) {
    try {
      results.prompts = registerCustomPrompts(registries.promptRegistry, customResources.prompts);
      logger.info(`Custom prompts registered: ${results.prompts.success.length} succeeded`);
    } catch (error) {
      logger.error("Failed to register custom prompts", { error });
    }
  }

  return results;
}
