/**
 * Execute Workflow Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { WorkflowInfo } from "../types.js";

/**
 * Base execute workflow tool description (without dynamic workflow info).
 * Use generateExecuteWorkflowDescription() for a description with workflow metadata.
 */
export const EXECUTE_WORKFLOW_TOOL_DESCRIPTION: ToolDescriptionData = {
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

/**
 * Generate a dynamic execute workflow description including available workflow metadata.
 * When workflows are provided, the description and workflowId parameter include
 * the list of available workflows for better LLM guidance.
 *
 * @param workflows - Optional list of available workflows to include in the description
 * @returns ToolDescriptionData with dynamic workflow information
 */
export function generateExecuteWorkflowDescription(
  workflows?: WorkflowInfo[],
): ToolDescriptionData {
  if (!workflows || workflows.length === 0) {
    return EXECUTE_WORKFLOW_TOOL_DESCRIPTION;
  }

  const workflowList = workflows
    .map(w => {
      const vars =
        w.variables && w.variables.length > 0
          ? ` (inputs: ${w.variables.map(v => v.name).join(", ")})`
          : "";
      return `  - ${w.id}: ${w.description}${vars}`;
    })
    .join("\n");

  const workflowIds = workflows.map(w => w.id);
  const idsWithQuotes = workflowIds.map(id => `'${id}'`).join(", ");

  return {
    ...EXECUTE_WORKFLOW_TOOL_DESCRIPTION,
    description: [
      "Execute a graph workflow dynamically. Use this tool to run a predefined workflow with the given input parameters.",
      "Supports both synchronous and asynchronous execution modes.",
      "",
      "Available workflows:",
      workflowList,
    ].join("\n"),
    parameters: EXECUTE_WORKFLOW_TOOL_DESCRIPTION.parameters.map(p =>
      p.name === "workflowId"
        ? {
            ...p,
            description: `The ID of the workflow to execute. Options: ${idsWithQuotes}`,
          }
        : p,
    ),
    tips: [
      ...(EXECUTE_WORKFLOW_TOOL_DESCRIPTION.tips ?? []),
      `Available workflow IDs: ${idsWithQuotes}`,
    ],
  };
}
