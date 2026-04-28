/**
 * Cancel Workflow Tool Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Cancel workflow parameters schema
 */
export const cancelWorkflowSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    taskId: {
      type: "string",
      description: "The task ID to cancel",
    },
  },
  required: ["taskId"],
};
