/**
 * Query Workflow Status Tool Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Query workflow status parameters schema
 */
export const queryWorkflowStatusSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    taskId: {
      type: "string",
      description: "The task ID to query status for",
    },
  },
  required: ["taskId"],
};
