/**
 * Query Workflow Status Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

/**
 * Query workflow status tool description
 */
export const QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "query_workflow_status",
  id: "builtin_query_workflow_status",
  type: "STATELESS",
  category: "code",
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
