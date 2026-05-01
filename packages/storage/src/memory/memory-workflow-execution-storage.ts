/**
 * In-Memory Workflow Execution Storage Adapter
 * Fast, isolated workflow execution storage for testing
 */

import type {
  WorkflowExecutionStorageMetadata,
  WorkflowExecutionListOptions,
  WorkflowExecutionStats,
  WorkflowExecutionStatus,
} from "@wf-agent/types";
import type { WorkflowExecutionStorageAdapter } from "../types/adapter/workflow-execution-adapter.js";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based workflow execution storage implementation
 * Implements WorkflowExecutionStorageAdapter interface with in-memory storage
 */
export class MemoryWorkflowExecutionStorage
  extends BaseMemoryStorage<WorkflowExecutionStorageMetadata, WorkflowExecutionListOptions>
  implements WorkflowExecutionStorageAdapter
{
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * List workflow execution IDs with filtering support
   */
  override async list(options?: WorkflowExecutionListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.workflowId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.workflowId === options.workflowId;
        });
      }

      if (options.status) {
        const statuses = Array.isArray(options.status) ? options.status : [options.status];
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && statuses.includes(entry.metadata.status);
        });
      }

      if (options.executionType) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.executionType === options.executionType;
        });
      }

      if (options.parentExecutionId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.parentExecutionId === options.parentExecutionId;
        });
      }

      if (options.startTimeFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.startTime >= options.startTimeFrom!;
        });
      }

      if (options.startTimeTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.startTime <= options.startTimeTo!;
        });
      }

      if (options.endTimeFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.endTime && entry.metadata.endTime >= options.endTimeFrom!;
        });
      }

      if (options.endTimeTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.endTime && entry.metadata.endTime <= options.endTimeTo!;
        });
      }

      if (options.tags && options.tags.length > 0) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          const metadataTags = entry?.metadata.tags || [];
          return options.tags!.some(tag => metadataTags.includes(tag));
        });
      }

      // Apply sorting
      if (options.sortBy) {
        ids.sort((a, b) => {
          const entryA = this.store.get(a);
          const entryB = this.store.get(b);

          if (!entryA || !entryB) return 0;

          let comparison = 0;
          switch (options.sortBy) {
            case "startTime":
              comparison = entryA.metadata.startTime - entryB.metadata.startTime;
              break;
            case "endTime":
              const timeA = entryA.metadata.endTime ?? 0;
              const timeB = entryB.metadata.endTime ?? 0;
              comparison = timeA - timeB;
              break;
            case "updatedAt":
              // Use endTime or startTime as updatedAt proxy
              const updatedA = entryA.metadata.endTime ?? entryA.metadata.startTime;
              const updatedB = entryB.metadata.endTime ?? entryB.metadata.startTime;
              comparison = updatedA - updatedB;
              break;
          }

          return options.sortOrder === "desc" ? -comparison : comparison;
        });
      }
    }

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? ids.length;
      ids = ids.slice(offset, offset + limit);
    }

    return ids;
  }

  /**
   * Update workflow execution status
   */
  async updateExecutionStatus(executionId: string, status: WorkflowExecutionStatus): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const entry = this.store.get(executionId);
    if (!entry) {
      throw new Error(`Workflow execution not found: ${executionId}`);
    }

    // Update status
    entry.metadata.status = status;
    
    // Update end time if terminal state
    if (["COMPLETED", "FAILED", "CANCELLED"].includes(status)) {
      entry.metadata.endTime = Date.now();
    }

    this.store.set(executionId, entry);
  }

  /**
   * Get workflow execution statistics (override base class method)
   */
  override getStats(): { count: number; totalSize: number } {
    return super.getStats();
  }

  /**
   * Get detailed workflow execution statistics
   */
  async getExecutionStats(options?: { workflowId?: string }): Promise<WorkflowExecutionStats> {
    this.ensureInitialized();
    await this.simulateLatency();

    let entries = Array.from(this.store.values());

    // Apply filter if provided
    if (options?.workflowId) {
      entries = entries.filter(e => e.metadata.workflowId === options.workflowId);
    }

    // Calculate statistics
    const byStatus: Record<string, number> = {};
    const byType: Record<string, number> = {};
    const byWorkflow: Record<string, number> = {};

    for (const entry of entries) {
      const status = entry.metadata.status;
      byStatus[status] = (byStatus[status] || 0) + 1;

      const type = entry.metadata.executionType || "UNKNOWN";
      byType[type] = (byType[type] || 0) + 1;

      const workflowId = entry.metadata.workflowId;
      byWorkflow[workflowId] = (byWorkflow[workflowId] || 0) + 1;
    }

    return {
      total: entries.length,
      byStatus: byStatus as Record<any, number>,
      byType: byType as Record<any, number>,
      byWorkflow,
    };
  }
}
