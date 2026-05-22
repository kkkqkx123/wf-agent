/**
 * Tool Visibility Node Handler
 * Handles TOOL_VISIBILITY nodes to manage tool permissions at runtime
 */

import type { RuntimeNode, ToolVisibilityNodeConfig } from "@wf-agent/types";
import type { ToolPermissionManager } from "../../../../core/coordinators/tool-permission-manager.js";
import type { RejectionMessageBuilder } from "../../../../core/coordinators/rejection-message-builder.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger();

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
 * @param node - Node definition
 * @param context - Handler context
 * @returns Execution result
 */
export async function toolVisibilityHandler(
  node: RuntimeNode,
  context: ToolVisibilityHandlerContext,
): Promise<import("@wf-agent/types").ToolVisibilityNodeOutput> {
  const config = node.config as ToolVisibilityNodeConfig;

  try {
    logger.info("Executing TOOL_VISIBILITY node", {
      nodeId: node.id,
      action: config.action,
      toolIds: config.toolIds,
      reason: config.reason,
    });

    if (config.action === "block") {
      context.permissionManager.disableTools(config.toolIds, config.reason, node.id);
      logger.info("Tools blocked", {
        nodeId: node.id,
        toolIds: config.toolIds,
        reason: config.reason,
      });
    } else if (config.action === "unblock") {
      context.permissionManager.enableTools(config.toolIds, config.reason, node.id);
      logger.info("Tools unblocked", {
        nodeId: node.id,
        toolIds: config.toolIds,
      });
    } else {
      throw new Error(`Unknown action: ${(config as any).action}`);
    }

    return {
      action: config.action,
      toolIds: config.toolIds,
    };
  } catch (error) {
    logger.error("TOOL_VISIBILITY node execution failed", {
      nodeId: node.id,
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}
