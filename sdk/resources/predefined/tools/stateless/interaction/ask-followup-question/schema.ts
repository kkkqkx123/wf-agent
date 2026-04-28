/**
 * The `ask_followup_question` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * ask_followup_question tool parameter Schema
 */
export const askFollowupQuestionSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    question: {
      type: "string",
      description: "Clear, specific question that captures the missing information you need",
    },
    follow_up: {
      type: "array",
      description:
        "Required list of 2-4 suggested responses; each suggestion must be a complete, actionable answer and may include a mode switch",
      items: {
        type: "object",
        properties: {
          text: {
            type: "string",
            description: "Suggested answer the user can pick",
          },
          mode: {
            type: "string",
            nullable: true,
            description:
              "Optional mode slug to switch to if this suggestion is chosen (e.g., code, architect)",
          },
        },
        required: ["text"],
      },
      minItems: 1,
      maxItems: 4,
    },
  },
  required: ["question", "follow_up"],
};
