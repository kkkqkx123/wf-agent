/**
 * WorkflowStorageManager - Workflow Storage Manager
 * Responsible for persisting, loading, and removing workflow definitions from storage.
 *
 * This module only exports class definitions; instances are managed uniformly through DI container.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * WorkflowStorageManager
 * Extracted persistence logic from WorkflowRegistry to isolate storage concerns.
 */
export class WorkflowStorageManager {
  constructor(private storageAdapter: WorkflowStorageAdapter | null) {}

  /**
   * Persist workflow to storage (if adapter is available)
   * @param workflow Workflow template to persist
   */
  async persist(workflow: WorkflowTemplate): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping workflow persistence");
      return;
    }

    try {
      // Serialize workflow to bytes
      const encoder = new TextEncoder();
      const data = encoder.encode(JSON.stringify(workflow));

      // Create metadata matching WorkflowStorageMetadata interface
      const metadata = {
        workflowId: workflow.id,
        name: workflow.name,
        type: workflow.type,
        version: workflow.version,
        createdAt: workflow.createdAt,
        updatedAt: workflow.updatedAt,
        description: workflow.description || "",
        tags: workflow.metadata?.tags || [],
        category: workflow.metadata?.category || "",
        author: workflow.metadata?.author || "",
        nodeCount: workflow.nodes.length,
        edgeCount: workflow.edges.length,
      };

      await this.storageAdapter.save(workflow.id, data, metadata);
      logger.debug("Workflow persisted successfully", { workflowId: workflow.id });
    } catch (error) {
      logger.error("Failed to persist workflow", {
        workflowId: workflow.id,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Remove workflow from storage
   * @param workflowId Workflow ID to remove
   */
  async remove(workflowId: string): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
      await this.storageAdapter.delete(workflowId);
      logger.debug("Workflow removed from storage", { workflowId });
    } catch (error) {
      logger.error("Failed to remove workflow from storage", {
        workflowId,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Load workflow from storage
   * @param workflowId Workflow ID to load
   * @returns Workflow template or null
   */
  async load(workflowId: string): Promise<WorkflowTemplate | null> {
    if (!this.storageAdapter) {
      return null;
    }

    try {
      const data = await this.storageAdapter.load(workflowId);
      if (!data) {
        return null;
      }

      const decoder = new TextDecoder();
      const json = decoder.decode(data);
      return JSON.parse(json) as WorkflowTemplate;
    } catch (error) {
      logger.error("Failed to load workflow from storage", {
        workflowId,
        error: getErrorMessage(error),
      });
      return null;
    }
  }

  /**
   * Initialize workflows map from storage.
   * Loads all workflow definitions from storage into the provided map.
   * @param workflows Map to populate with loaded workflows
   */
  async initialize(workflows: Map<string, WorkflowTemplate>): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping initialization from storage");
      return;
    }

    try {
      // Get all workflow IDs from storage
      const ids = await this.storageAdapter.list();

      logger.info("Initializing workflows from storage", {
        count: ids.length,
        storageType: this.storageAdapter.constructor.name,
      });

      for (const id of ids) {
        try {
          const workflow = await this.load(id);
          if (workflow) {
            workflows.set(id, workflow);
          }
        } catch (error) {
          logger.error("Failed to load workflow from storage", {
            workflowId: id,
            error: getErrorMessage(error),
          });
        }
      }

      logger.info("Workflow initialization complete", {
        loadedCount: workflows.size,
      });
    } catch (error) {
      logger.error("Failed to initialize workflows from storage", {
        error: getErrorMessage(error),
      });
    }
  }
}
