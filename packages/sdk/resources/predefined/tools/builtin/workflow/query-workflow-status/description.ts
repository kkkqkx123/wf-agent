/**
 * Query Workflow Status Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { WorkflowInfo } from "../types.js";

/**
 * Base query workflow status tool description (without dynamic workflow info).
 * Use generateQueryWorkflowStatusDescription() for a description with workflow metadata.
 */
export const QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "builtin_query_workflow_status",
  type: "STATELESS",
  category: "workflow",
  description:
    "Query the status of an asynchronously submitted workflow task. Use this tool to check if a background workflow has completed, failed, or is still running.",
  parameters: [
    {
      name: "taskId",
      type: "string",
      required: true,
      description: "The task ID returned from an asynchronous execute_workflow call",
    },
  ],
  tips: [
    "Use this tool to poll for completion of async workflows",
    "The status field indicates the current state of the task",
  ],
};

/**
 * Generate a dynamic query workflow status description including available workflow metadata.
 *
 * @param workflows - Optional list of available workflows to include in the description
 * @returns ToolDescriptionData with dynamic workflow information
 */
export function generateQueryWorkflowStatusDescription(
  workflows?: WorkflowInfo[],
): ToolDescriptionData {
  if (!workflows || workflows.length === 0) {
    return QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION;
  }

  const workflowIds = workflows.map(w => w.id);
  const idsWithQuotes = workflowIds.map(id => `'${id}'`).join(", ");

  return {
    ...QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION,
    tips: [
      ...(QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION.tips ?? []),
      `Available workflow IDs: ${idsWithQuotes}`,
    ],
  };
}
