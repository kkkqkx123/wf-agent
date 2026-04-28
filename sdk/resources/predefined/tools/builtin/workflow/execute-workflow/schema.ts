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
      description: "Input parameters for the workflow",
      additionalProperties: true,
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
