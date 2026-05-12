/**
 * Execute Workflow Tool Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Execute workflow parameters schema
 */
export const executeWorkflowSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    workflowId: {
      type: "string",
      description: "The ID of the workflow to execute",
    },
    input: {
      type: "object",
      description: "Input parameters for the workflow (variables)",
      additionalProperties: true,
    },
    messageContexts: {
      type: "object",
      description: "Named message contexts to pass to the workflow. Keys are context IDs, values are message arrays.",
      additionalProperties: {
        type: "array",
        items: {
          type: "object",
        },
      },
    },
    waitForCompletion: {
      type: "boolean",
      description: "Whether to wait for the workflow to complete (default: true)",
    },
    timeout: {
      type: "number",
      description: "Timeout in milliseconds",
    },
  },
  required: ["workflowId"],
};
