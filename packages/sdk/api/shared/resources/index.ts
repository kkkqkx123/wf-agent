/**
 * Resource Index - Unified Export of Resource Management APIs
 * Provides a unified access point for all resource management APIs.
 */

// General Resources API Base Classes and Interfaces
import {
  QueryableResourceAPI,
  SimplifiedCrudResourceAPI,
  BaseResourceAPI,
  type WritableResourceAPI,
  type ClearableResourceAPI,
} from "./generic-resource-api.js";

// Event Resource Management
import { EventResourceAPI, type EventStats } from "./events/event-resource-api.js";
import type {
  ExecutionTimeline,
  ExecutionTimelinePhase,
  ExecutionTimelineSummary,
} from "./events/event-resource-api.js";

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

// Metrics Resource Management
import { MetricsResourceAPI } from "./metrics/metrics-resource-api.js";

// Task Resource Management
import {
  TaskResourceAPI,
  type TaskFilter,
  type TaskSummary,
  type TaskStats,
} from "./tasks/task-resource-api.js";

// Workflow Graph Query API
import {
  WorkflowGraphQueryAPI,
  type WorkflowGraphSummary,
  type GraphNodeStats,
  type GraphEdgeStats,
  type NodeNeighbors,
} from "./graphs/workflow-graph-query-api.js";

// Storage Diagnostics API
import {
  StorageDiagnosticsAPI,
  type StorageAdapterHealth,
  type StorageItemCounts,
  type StorageDiagnosticsReport,
} from "./diagnostics/storage-diagnostics-api.js";

// Unified Search API
import {
  SearchAPI,
  type SearchResourceType,
  type SearchOptions,
  type SearchResultItem,
  type SearchResult,
} from "./search/search-api.js";

// Dependency Management
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

// Reexport all resource management APIs
export {
  QueryableResourceAPI,
  SimplifiedCrudResourceAPI,
  BaseResourceAPI,
  type WritableResourceAPI,
  type ClearableResourceAPI,
};
export {
  EventResourceAPI,
  type EventStats,
  type ExecutionTimeline,
  type ExecutionTimelinePhase,
  type ExecutionTimelineSummary,
};
export { ToolRegistryAPI };
export { ScriptRegistryAPI };
export { LLMProfileRegistryAPI };
export { SkillRegistryAPI, type SkillFilter, type SkillLoadOptions };
export { MetricsResourceAPI };
export { TaskResourceAPI, type TaskFilter, type TaskSummary, type TaskStats };
export {
  WorkflowGraphQueryAPI,
  type WorkflowGraphSummary,
  type GraphNodeStats,
  type GraphEdgeStats,
  type NodeNeighbors,
};
export {
  StorageDiagnosticsAPI,
  type StorageAdapterHealth,
  type StorageItemCounts,
  type StorageDiagnosticsReport,
};
export {
  SearchAPI,
  type SearchResourceType,
  type SearchOptions,
  type SearchResultItem,
  type SearchResult,
};

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
    metrics: new MetricsResourceAPI(dependencies),
    tasks: new TaskResourceAPI(dependencies),
    graphs: new WorkflowGraphQueryAPI(dependencies),
    diagnostics: new StorageDiagnosticsAPI(dependencies),
    search: new SearchAPI(dependencies),
  };
}

/**
 * Shared Resource Management API Type Definition
 */
export type SharedResourceAPIs = ReturnType<typeof createSharedResourceAPIs>;
