/**
 * Cancel Workflow Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

/**
 * Cancel workflow tool description
 */
export const CANCEL_WORKFLOW_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "cancel_workflow",
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
