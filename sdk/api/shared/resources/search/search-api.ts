/**
 * SearchAPI - Unified Search API
 * Provides cross-resource search across workflows, executions, tasks, checkpoints, and events.
 *
 * Design Principles:
 * - Leverages existing registry search methods
 * - Unified result format with type discrimination
 * - Supports filtering by resource type and search options
 * - Non-destructive: read-only access to all resources
 */

import type { ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "SearchAPI" });

/**
 * Resource types that can be searched
 */
export type SearchResourceType =
  | "workflow"
  | "execution"
  | "task"
  | "checkpoint"
  | "event"
  | "agent_loop";

/**
 * Search options
 */
export interface SearchOptions {
  /** Resource types to search (default: all) */
  types?: SearchResourceType[];
  /** Maximum results per resource type (default: 20) */
  limitPerType?: number;
  /** Maximum total results (default: 100) */
  maxTotal?: number;
}

/**
 * Base search result item
 */
export interface SearchResultItem {
  /** Matched resource ID */
  id: ID;
  /** Resource type */
  type: SearchResourceType;
  /** Display label */
  label: string;
  /** Relevance score (0-1, higher is better match) */
  score: number;
  /** Matched field values for highlighting */
  matches: Record<string, string>;
}

/**
 * Unified search result
 */
export interface SearchResult {
  /** Search query that produced these results */
  query: string;
  /** Result items */
  items: SearchResultItem[];
  /** Result count by type */
  byType: Partial<Record<SearchResourceType, number>>;
  /** Total result count */
  total: number;
  /** Truncated flag if maxTotal limit was hit */
  truncated: boolean;
}

/**
 * Workflow match result from registry search
 */
interface WorkflowMatch {
  id: ID;
  name: string;
  description?: string;
}

/**
 * SearchAPI - Unified Search API
 */
export class SearchAPI {
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager) {
    this.deps = deps;
    logger.info("SearchAPI initialized");
  }

  /**
   * Execute a unified search across resource types
   * @param query Search query string
   * @param options Search options
   * @returns Unified search result
   */
  async search(query: string, options?: SearchOptions): Promise<SearchResult> {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return { query, items: [], byType: {}, total: 0, truncated: false };
    }

    const types = options?.types ?? this.getAllTypes();
    const limitPerType = options?.limitPerType ?? 20;
    const maxTotal = options?.maxTotal ?? 100;
    const allItems: SearchResultItem[] = [];
    const byType: Partial<Record<SearchResourceType, number>> = {};
    let truncated = false;

    // Search each resource type in parallel
    const searchPromises: Promise<void>[] = [];

    if (types.includes("workflow")) {
      searchPromises.push(this.searchWorkflows(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.workflow = items.length;
      }));
    }

    if (types.includes("execution")) {
      searchPromises.push(this.searchExecutions(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.execution = items.length;
      }));
    }

    if (types.includes("task")) {
      searchPromises.push(this.searchTasks(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.task = items.length;
      }));
    }

    if (types.includes("checkpoint")) {
      searchPromises.push(this.searchCheckpoints(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.checkpoint = items.length;
      }));
    }

    if (types.includes("event")) {
      searchPromises.push(this.searchEvents(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.event = items.length;
      }));
    }

    if (types.includes("agent_loop")) {
      searchPromises.push(this.searchAgentLoops(normalizedQuery, limitPerType).then(items => {
        allItems.push(...items);
        byType.agent_loop = items.length;
      }));
    }

    await Promise.all(searchPromises);

    // Sort by score descending
    allItems.sort((a, b) => b.score - a.score);

    // Apply max total limit
    if (allItems.length > maxTotal) {
      allItems.length = maxTotal;
      truncated = true;
    }

    return {
      query,
      items: allItems,
      byType,
      total: allItems.length,
      truncated,
    };
  }

  /**
   * Search workflows by keyword
   */
  private async searchWorkflows(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const registry = this.deps.getWorkflowRegistry();
      const summaries: WorkflowMatch[] = await registry.search(query);

      return summaries.slice(0, limit).map(s => ({
        id: s.id,
        type: "workflow" as SearchResourceType,
        label: s.name,
        score: this.computeScore(query, s.name, s.description),
        matches: this.computeMatches(query, { name: s.name, description: s.description }),
      }));
    } catch (error) {
      logger.warn("Workflow search failed", { error });
      return [];
    }
  }

  /**
   * Search executions by keyword
   */
  private async searchExecutions(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const registry = this.deps.getWorkflowExecutionRegistry();
      const all = registry.getAll();
      const items: SearchResultItem[] = [];

      for (const entity of all) {
        if (items.length >= limit) break;

        const id = entity.id;
        const workflowId = entity.getWorkflowId();
        const idStr = String(id);
        const wfStr = String(workflowId);

        if (idStr.toLowerCase().includes(query) || wfStr.toLowerCase().includes(query)) {
          items.push({
            id,
            type: "execution" as SearchResourceType,
            label: `Execution ${idStr.substring(0, 12)}...`,
            score: idStr.toLowerCase() === query ? 1 : 0.5,
            matches: { id: idStr, workflowId: wfStr },
          });
        }
      }

      return items;
    } catch (error) {
      logger.warn("Execution search failed", { error });
      return [];
    }
  }

  /**
   * Search tasks by keyword
   */
  private async searchTasks(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const registry = this.deps.getTaskRegistry();
      const allTasks = registry.getAll();
      const items: SearchResultItem[] = [];

      for (const task of allTasks) {
        if (items.length >= limit) break;

        const taskId = String(task.id);
        const statusStr = String(task.status ?? "");

        if (
          taskId.toLowerCase().includes(query) ||
          statusStr.toLowerCase().includes(query)
        ) {
          items.push({
            id: task.id,
            type: "task" as SearchResourceType,
            label: `Task ${taskId.substring(0, 12)}...`,
            score: taskId.toLowerCase() === query ? 1 : 0.6,
            matches: { id: taskId, status: statusStr },
          });
        }
      }

      return items;
    } catch (error) {
      logger.warn("Task search failed", { error });
      return [];
    }
  }

  /**
   * Search checkpoints by keyword
   */
  private async searchCheckpoints(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const stateManager = this.deps.getCheckpointStateManager();
      const checkpointIds = await stateManager.list();
      const items: SearchResultItem[] = [];

      for (const cpId of checkpointIds) {
        if (items.length >= limit) break;

        const checkpoint = await stateManager.get(cpId);
        if (!checkpoint) continue;

        const execId = String(checkpoint.executionId);
        const wfId = String(checkpoint.workflowId);
        const desc = checkpoint.metadata?.description ?? "";

        if (
          cpId.toLowerCase().includes(query) ||
          execId.toLowerCase().includes(query) ||
          wfId.toLowerCase().includes(query) ||
          desc.toLowerCase().includes(query)
        ) {
          items.push({
            id: cpId,
            type: "checkpoint" as SearchResourceType,
            label: desc ? `Checkpoint: ${desc}` : `Checkpoint ${cpId.substring(0, 12)}...`,
            score: cpId.toLowerCase() === query ? 1 : 0.4,
            matches: { id: cpId, executionId: execId, workflowId: wfId, description: desc },
          });
        }
      }

      return items;
    } catch (error) {
      logger.warn("Checkpoint search failed", { error });
      return [];
    }
  }

  /**
   * Search events by keyword
   */
  private async searchEvents(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const eventManager = this.deps.getEventManager();
      const items: SearchResultItem[] = [];

      // Search through metrics-collected events
      const metricsCollector = eventManager.getMetricsCollector();
      const summary = metricsCollector.generateSummary();

      for (const [eventType] of summary.byEventType.entries()) {
        if (items.length >= limit) break;

        if (eventType.toLowerCase().includes(query)) {
          items.push({
            id: eventType,
            type: "event" as SearchResourceType,
            label: `Event: ${eventType}`,
            score: eventType.toLowerCase() === query ? 1 : 0.7,
            matches: { type: eventType },
          });
        }
      }

      return items;
    } catch (error) {
      logger.warn("Event search failed", { error });
      return [];
    }
  }

  /**
   * Search agent loops by keyword
   */
  private async searchAgentLoops(query: string, limit: number): Promise<SearchResultItem[]> {
    try {
      const registry = this.deps.getAgentLoopRegistry();
      const all = registry.getAll();
      const items: SearchResultItem[] = [];

      for (const entity of all) {
        if (items.length >= limit) break;

        const id = String(entity.id);
        const status = entity.getStatus();

        if (id.toLowerCase().includes(query)) {
          items.push({
            id: entity.id,
            type: "agent_loop" as SearchResourceType,
            label: `Agent Loop ${id.substring(0, 12)}...`,
            score: id.toLowerCase() === query ? 1 : 0.5,
            matches: { id, status: String(status) },
          });
        }
      }

      return items;
    } catch (error) {
      logger.warn("Agent loop search failed", { error });
      return [];
    }
  }

  /**
   * Get all searchable resource types
   */
  private getAllTypes(): SearchResourceType[] {
    return ["workflow", "execution", "task", "checkpoint", "event", "agent_loop"];
  }

  /**
   * Compute relevance score based on query matching
   */
  private computeScore(query: string, name: string, description?: string): number {
    const nameLower = name.toLowerCase();
    let score = 0;

    if (nameLower === query) {
      score = 1;
    } else if (nameLower.startsWith(query)) {
      score = 0.9;
    } else if (nameLower.includes(query)) {
      score = 0.7;
    } else if (description?.toLowerCase().includes(query)) {
      score = 0.4;
    }

    return score;
  }

  /**
   * Compute matched fields for highlighting
   */
  private computeMatches(
    query: string,
    fields: Record<string, string | undefined>,
  ): Record<string, string> {
    const matches: Record<string, string> = {};
    for (const [key, value] of Object.entries(fields)) {
      if (value && value.toLowerCase().includes(query)) {
        matches[key] = value;
      }
    }
    return matches;
  }
}