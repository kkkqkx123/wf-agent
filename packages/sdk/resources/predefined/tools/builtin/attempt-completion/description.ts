import type { ToolDescriptionData } from "@wf-agent/types";

export const ATTEMPT_COMPLETION_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "attempt_completion",
  type: "STATELESS",
  category: "interaction",
  description: `Complete the current agent task and signal that no further action is needed.

Use this tool when the task is complete — the agent's work is done, results are ready, and the loop should end.

Parameters:
- data: Output records. Each key appends a record to a matching array variable (tail insert). For example, {"judges":{"iteration":1,"score":8}} appends one judge record.
- variables: State changes. Each key-value pair sets a workflow variable directly. For example, {"complete":true} sets the "complete" variable.

Best Practices:
- Provide a concise final assistant message summarizing what was done — that message IS the result text
- Use data for persistent output records that accumulate across iterations (e.g., review results, error logs)
- Use variables for workflow control flags (e.g., complete, retryCount)
- Keep each data value as a single object, not an array — one record per call
- For complex batch operations, consider dedicated tools`,
  parameters: [
    {
      name: "data",
      type: "object",
      required: false,
      description: "Output records. Each key appends its value to the matching array variable.",
    },
    {
      name: "variables",
      type: "object",
      required: false,
      description: "State changes. Each key-value pair sets a workflow variable directly.",
    },
  ],
  tips: [
    "Always call attempt_completion when the task goal has been achieved",
    "The agent's final assistant message serves as the result text — make it clear and informative",
    "Use data for accumulated records (e.g., review findings, change logs)",
    "Use variables for boolean flags or counters that control loop behavior",
    "Do NOT continue adding tool calls after calling attempt_completion",
  ],
};
