/**
 * Tool Visibility Node Handler
 * Handles TOOL_VISIBILITY nodes to manage tool permissions at runtime
 */

import type { RuntimeNode, ToolVisibilityNodeConfig, WorkflowExecution } from "@wf-agent/types";
import type { ToolPermissionManager } from "../../../../core/coordinators/tool-permission-manager.js";
import type { RejectionMessageBuilder } from "../../../../core/coordinators/rejection-message-builder.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Tool visibility execution result
 */
export interface ToolVisibilityExecutionResult {
  /** Execution status */
  status: 'COMPLETED' | 'FAILED';
  /** Updated enabled tools (if any) */
  enabledTools?: string[];
  /** Updated disabled tools (if any) */
  disabledTools?: string[];
  /** User message hint (if configured) */
  userMessageHint?: string | null;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Tool visibility handler context
 */
export interface ToolVisibilityHandlerContext {
  /** Tool Permission Manager */
  permissionManager: ToolPermissionManager;
  /** Rejection Message Builder */
  rejectionBuilder: RejectionMessageBuilder;
}

/**
 * Tool visibility node handler
 * @param workflowExecution - Workflow execution instance
 * @param node - Node definition
 * @param context - Handler context
 * @returns Execution result
 */
export async function toolVisibilityHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: ToolVisibilityHandlerContext,
): Promise<ToolVisibilityExecutionResult> {
  const config = node.config as ToolVisibilityNodeConfig;
  const startTime = now();

  try {
    logger.info('Executing TOOL_VISIBILITY node', {
      nodeId: node.id,
      action: config.action,
      toolIds: config.toolIds,
      reason: config.reason
    });

    let enabledTools: string[] = [];
    let disabledTools: string[] = [];

    if (config.action === 'block') {
      // Disable the specified tools
      context.permissionManager.disableTools(
        config.toolIds,
        config.reason,
        node.id
      );
      disabledTools = config.toolIds;
      
      logger.info('Tools blocked', {
        nodeId: node.id,
        toolIds: config.toolIds,
        reason: config.reason
      });
    } else if (config.action === 'unblock') {
      // Enable the specified tools
      context.permissionManager.enableTools(
        config.toolIds,
        config.reason,
        node.id
      );
      enabledTools = config.toolIds;
      
      logger.info('Tools unblocked', {
        nodeId: node.id,
        toolIds: config.toolIds
      });
    } else {
      throw new Error(`Unknown action: ${(config as any).action}`);
    }

    // Build user message hint (if configured)
    const userMessageHint = context.rejectionBuilder.buildUserMessageHint(
      enabledTools,
      disabledTools
    );

    const executionTime = now() - startTime;

    return {
      status: 'COMPLETED',
      enabledTools: enabledTools.length > 0 ? enabledTools : undefined,
      disabledTools: disabledTools.length > 0 ? disabledTools : undefined,
      userMessageHint,
      executionTime
    };
  } catch (error) {
    const executionTime = now() - startTime;
    
    logger.error('TOOL_VISIBILITY node execution failed', {
      nodeId: node.id,
      error: error instanceof Error ? error.message : String(error)
    });

    return {
      status: 'FAILED',
      error: error instanceof Error ? error : new Error(String(error)),
      executionTime
    };
  }
}
