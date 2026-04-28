/**
 * Resource Index - Unified Export of Resource Management APIs
 * Provides a unified access point for all resource management APIs.
 */

// General Resources API Base Classes and Interfaces
import {
  ReadonlyResourceAPI,
  CrudResourceAPI,
  BaseResourceAPI,
  type ReadableResourceAPI,
  type WritableResourceAPI,
  type ClearableResourceAPI,
} from "./generic-resource-api.js";

// Checkpoint resource management
import { CheckpointResourceAPI } from "../../workflow/resources/checkpoints/checkpoint-resource-api.js";

// Message Resource Management
import {
  MessageResourceAPI,
  type MessageFilter,
  type MessageStats,
} from "../../workflow/resources/messages/message-resource-api.js";

// Variable Resource Management
import {
  VariableResourceAPI,
  type VariableFilter,
  type VariableDefinition,
} from "../../workflow/resources/variables/variable-resource-api.js";

// Trigger Resource Management
import { TriggerResourceAPI } from "../../workflow/resources/triggers/trigger-resource-api.js";

// Event Resource Management
import { EventResourceAPI, type EventStats } from "./events/event-resource-api.js";

// Workflow Resource Management
import { WorkflowRegistryAPI } from "../../workflow/resources/workflows/workflow-registry-api.js";

// Thread resource management
import { ThreadRegistryAPI } from "../../workflow/resources/threads/thread-registry-api.js";

// Tool Resource Management
import { ToolRegistryAPI } from "./tools/tool-registry-api.js";

// Script Resource Management
import { ScriptRegistryAPI } from "./scripts/script-registry-api.js";

// Node Template Resource Management
import { NodeRegistryAPI } from "../../workflow/resources/templates/node-template-registry-api.js";

// Trigger Template Resource Management
import { TriggerTemplateRegistryAPI } from "../../workflow/resources/templates/trigger-template-registry-api.js";

// Profile Resource Management
import { LLMProfileRegistryAPI } from "./llm/llm-profile-registry-api.js";

// User Interaction Resource Management
import {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "../../workflow/resources/user-interaction/user-interaction-resource-api.js";

// Human Relay Resource Management
import {
  HumanRelayResourceAPI,
  type HumanRelayConfig,
  type HumanRelayFilter,
} from "../../workflow/resources/human-relay/human-relay-resource-api.js";

// Skill Resource Management
import {
  SkillRegistryAPI,
  type SkillFilter,
  type SkillLoadOptions,
} from "./skills/skill-registry-api.js";

// Dependency Management
import type { APIDependencyManager } from "../core/sdk-dependencies.js";

// Reexport all resource management APIs
export {
  ReadonlyResourceAPI,
  CrudResourceAPI,
  BaseResourceAPI,
  type ReadableResourceAPI,
  type WritableResourceAPI,
  type ClearableResourceAPI,
};
export { CheckpointResourceAPI };
export { MessageResourceAPI, type MessageFilter, type MessageStats };
export { VariableResourceAPI, type VariableFilter, type VariableDefinition };
export { TriggerResourceAPI };
export { EventResourceAPI, type EventStats };
export { WorkflowRegistryAPI };
export { ThreadRegistryAPI };
export { ToolRegistryAPI };
export { ScriptRegistryAPI };
export { NodeRegistryAPI };
export { TriggerTemplateRegistryAPI };
export { LLMProfileRegistryAPI };
export { UserInteractionResourceAPI, type UserInteractionConfig, type UserInteractionFilter };
export { HumanRelayResourceAPI, type HumanRelayConfig, type HumanRelayFilter };
export { SkillRegistryAPI, type SkillFilter, type SkillLoadOptions };

/**
 * Create a factory function for all resource management API instances
 */
export function createResourceAPIs(dependencies: APIDependencyManager) {
  return {
    checkpoints: new CheckpointResourceAPI(),
    messages: new MessageResourceAPI(),
    variables: new VariableResourceAPI(),
    triggers: new TriggerResourceAPI(),
    events: new EventResourceAPI(dependencies),
    workflows: new WorkflowRegistryAPI(dependencies),
    threads: new ThreadRegistryAPI(dependencies),
    tools: new ToolRegistryAPI(dependencies),
    scripts: new ScriptRegistryAPI(dependencies),
    nodeTemplates: new NodeRegistryAPI(dependencies),
    triggerTemplates: new TriggerTemplateRegistryAPI(dependencies),
    profiles: new LLMProfileRegistryAPI(),
    userInteractions: new UserInteractionResourceAPI(dependencies),
    humanRelay: new HumanRelayResourceAPI(dependencies),
    skills: new SkillRegistryAPI(dependencies),
  };
}

/**
 * Resource Management API Type Definition
 */
export type ResourceAPIs = ReturnType<typeof createResourceAPIs>;
