/**
 * Execute Workflow Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

/**
 * Execute workflow tool description
 */
export const EXECUTE_WORKFLOW_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "execute_workflow",
  id: "builtin_execute_workflow",
  type: "STATELESS",
  category: "workflow",
  description:
    "Execute a graph workflow dynamically. Use this tool to run a predefined workflow with the given input parameters. Supports both synchronous and asynchronous execution modes.",
  parameters: [
    {
      name: "workflowId",
      type: "string",
      required: true,
      description: "The ID of the workflow to execute",
    },
    {
      name: "input",
      type: "object",
      required: false,
      description: "Input parameters for the workflow",
    },
    {
      name: "waitForCompletion",
      type: "boolean",
      required: false,
      description: "Whether to wait for the workflow to complete (default: true)",
      defaultValue: true,
    },
    {
      name: "timeout",
      type: "number",
      required: false,
      description: "Timeout in milliseconds",
    },
  ],
  tips: [
    "Use waitForCompletion=false for background execution",
    "The tool returns a taskId for async execution that can be queried with query_workflow_status",
  ],
};
