/**
 * Workflow storage utilities - Module-level functions for workflow persistence.
 * Extracted from WorkflowRegistry to isolate storage concerns.
 * Dependencies (WorkflowStorageAdapter) are passed as function parameters.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist workflow to storage (if adapter is available)
 * @param workflow Workflow template to persist
 * @param adapter Storage adapter or null
 */
export async function persistWorkflow(
  workflow: WorkflowTemplate,
  adapter?: WorkflowStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping workflow persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(workflow));

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

    await adapter.save(workflow.id, data, metadata);
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
 * @param adapter Storage adapter or null
 */
export async function removeWorkflow(
  workflowId: string,
  adapter?: WorkflowStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(workflowId);
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
 * @param adapter Storage adapter or null
 * @returns Workflow template or null
 */
export async function loadWorkflow(
  workflowId: string,
  adapter?: WorkflowStorageAdapter | null,
): Promise<WorkflowTemplate | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(workflowId);
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
 * @param adapter Storage adapter or null
 * @param workflows Map to populate with loaded workflows
 */
export async function initializeWorkflowsFromStorage(
  adapter: WorkflowStorageAdapter | null,
  workflows: Map<string, WorkflowTemplate>,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping initialization from storage");
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing workflows from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const workflow = await loadWorkflow(id, adapter);
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
