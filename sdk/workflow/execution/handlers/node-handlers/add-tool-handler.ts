/**
 * Tool adds a node processor
 * Responsible for adding the tool to the tool context
 *
 * Design principles:
 * - Only includes the core execution logic
 * - Depends on ToolContextCoordinator for tool management
 * - Returns the execution result
 */

import type { RuntimeNode, AddToolNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { ToolContextStore } from "../../../stores/tool-context-store.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { buildToolAddedEvent } from "../../../../core/utils/event/builders/tool-events.js";
import type { ToolRegistry } from "../../../../core/registry/tool-registry.js";
import type { ToolVisibilityCoordinator } from "../../coordinators/tool-visibility-coordinator.js";

/**
 * Tool adds node execution results
 */
export interface AddToolExecutionResult {
  /** Execution Status */
  status: "COMPLETED" | "FAILED";
  /** Number of successfully added tools */
  addedCount?: number;
  /** The number of tools that were skipped (already existing and not overwritten) */
  skippedCount?: number;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Tool adds node processor context
 */
export interface AddToolHandlerContext {
  /** Tool Context Store */
  toolContextStore: ToolContextStore;
  /** Tool Services */
  toolService: ToolRegistry;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Workflow execution entity (used for tool visibility declarations) */
  workflowExecutionEntity?: WorkflowExecutionEntity;
  /** Tool Visibility Coordinator (for updating visibility declarations) */
  toolVisibilityCoordinator?: ToolVisibilityCoordinator;
}

/**
 * Tool adds a node processor
 * @param workflowExecution Workflow execution instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function addToolHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: AddToolHandlerContext,
): Promise<AddToolExecutionResult> {
  const config = node.config as AddToolNodeConfig;
  const startTime = now();

  try {
    // 1. Verify the tool ID
    const validToolIds: string[] = [];
    const invalidToolIds: string[] = [];

    for (const toolId of config.toolIds) {
      const tool = context.toolService.getTool(toolId);
      if (tool) {
        validToolIds.push(toolId);
      } else {
        invalidToolIds.push(toolId);
      }
    }

    if (invalidToolIds.length > 0) {
      throw new ExecutionError(`Invalid tool IDs: ${invalidToolIds.join(", ")}`, node.id);
    }

    // 2. Add tools to the context
    const scope = (config.scope as "EXECUTION" | "LOCAL") || "EXECUTION";
    const overwrite = config.overwrite || false;

    const addedCount = context.toolContextStore.addTools(
      workflowExecution.id,
      validToolIds,
      scope,
      overwrite,
      config.descriptionTemplate,
      config.metadata,
    );

    // Note: AvailableTools.dynamic synchronization with template is handled at a higher level
    // The ToolContextStore maintains the runtime state of available tools

    // 3. Calculate the number of tools that were skipped.
    const skippedCount = validToolIds.length - addedCount;

    // 4. Update tool visibility declaration (CRITICAL - makes tools visible to LLM)
    if (context.toolVisibilityCoordinator && context.workflowExecutionEntity && addedCount > 0) {
      await context.toolVisibilityCoordinator.addToolsDynamically(
        context.workflowExecutionEntity,
        validToolIds,
        scope,
      );
    }

    // 5. Triggering the tool to add an event
    await context.eventManager.emit(
      buildToolAddedEvent({
        workflowId: workflowExecution.workflowId,
        executionId: workflowExecution.id,
        nodeId: node.id,
        toolIds: validToolIds,
        scope,
        addedCount,
        skippedCount,
      }),
    );

    const endTime = now();

    return {
      status: "COMPLETED",
      addedCount,
      skippedCount,
      executionTime: diffTimestamp(startTime, endTime),
    };
  } catch (error) {
    const endTime = now();
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}
