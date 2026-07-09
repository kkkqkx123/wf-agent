import type { ToolParameterSchema } from "@wf-agent/types";

export const attemptCompletionSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    data: {
      type: "object",
      description: "Output records. Each value is appended to the matching array variable. Use single objects (not arrays) for one-at-a-time appends.",
      additionalProperties: true,
      optional: true,
    },
    variables: {
      type: "object",
      description: "State changes. Each key-value pair is written directly as a workflow variable (always set, not appended).",
      additionalProperties: true,
      optional: true,
    },
  },
  required: [],
};
