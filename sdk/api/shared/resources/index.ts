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

// Event Resource Management
import { EventResourceAPI, type EventStats } from "./events/event-resource-api.js";

// Tool Resource Management
import { ToolRegistryAPI } from "./tools/tool-registry-api.js";

// Script Resource Management
import { ScriptRegistryAPI } from "./scripts/script-registry-api.js";

// Profile Resource Management
import { LLMProfileRegistryAPI } from "./llm/llm-profile-registry-api.js";

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
export { EventResourceAPI, type EventStats };
export { ToolRegistryAPI };
export { ScriptRegistryAPI };
export { LLMProfileRegistryAPI };
export { SkillRegistryAPI, type SkillFilter, type SkillLoadOptions };

/**
 * Create a factory function for shared resource management API instances
 * Note: Workflow-specific resource APIs are exported from sdk/api/workflow
 */
export function createSharedResourceAPIs(dependencies: APIDependencyManager) {
  return {
    events: new EventResourceAPI(dependencies),
    tools: new ToolRegistryAPI(dependencies),
    scripts: new ScriptRegistryAPI(dependencies),
    profiles: new LLMProfileRegistryAPI(),
    skills: new SkillRegistryAPI(dependencies),
  };
}

/**
 * Shared Resource Management API Type Definition
 */
export type SharedResourceAPIs = ReturnType<typeof createSharedResourceAPIs>;
