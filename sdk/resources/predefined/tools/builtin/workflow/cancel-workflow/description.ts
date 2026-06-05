/**
 * Cancel Workflow Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { WorkflowInfo } from "../types.js";

/**
 * Base cancel workflow tool description (without dynamic workflow info).
 * Use generateCancelWorkflowDescription() for a description with workflow metadata.
 */
export const CANCEL_WORKFLOW_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "builtin_cancel_workflow",
  type: "STATELESS",
  category: "workflow",
  description:
    "Cancel a running workflow task. Use this tool to stop a background workflow execution.",
  parameters: [
    {
      name: "taskId",
      type: "string",
      required: true,
      description: "The task ID to cancel",
    },
  ],
  tips: [
    "Only async workflows can be cancelled",
    "The task must be in a running state to be cancelled",
  ],
};

/**
 * Generate a dynamic cancel workflow description including available workflow metadata.
 *
 * @param workflows - Optional list of available workflows to include in the description
 * @returns ToolDescriptionData with dynamic workflow information
 */
export function generateCancelWorkflowDescription(
  workflows?: WorkflowInfo[],
): ToolDescriptionData {
  if (!workflows || workflows.length === 0) {
    return CANCEL_WORKFLOW_TOOL_DESCRIPTION;
  }

  const workflowIds = workflows.map(w => w.id);
  const idsWithQuotes = workflowIds.map(id => `'${id}'`).join(", ");

  return {
    ...CANCEL_WORKFLOW_TOOL_DESCRIPTION,
    tips: [
      ...(CANCEL_WORKFLOW_TOOL_DESCRIPTION.tips ?? []),
      `Available workflow IDs: ${idsWithQuotes}`,
    ],
  };
}
