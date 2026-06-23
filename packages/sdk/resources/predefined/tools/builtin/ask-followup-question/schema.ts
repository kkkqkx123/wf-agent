/**
 * The `ask_followup_question` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * ask_followup_question tool parameter Schema
 *
 * Supports multiple questions (max 3), each with its own preset options (1-4 items).
 * Includes an additional information field for free-form user input.
 */
export const askFollowupQuestionSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    questions: {
      type: "array",
      description: "Array of 1-3 questions, each with its own preset options",
      items: {
        type: "object",
        properties: {
          text: {
            type: "string",
            description: "A clear, specific question to ask the user",
          },
          options: {
            type: "array",
            description: "Preset options for this specific question (1-4 options)",
            items: {
              type: "string",
              description: "An option that users can select as an answer",
            },
            minItems: 1,
            maxItems: 4,
          },
        },
        required: ["text", "options"],
      },
      minItems: 1,
      maxItems: 3, // Limit to 3 questions + 1 additional info = max 4 inputs
    },
    additionalInfoLabel: {
      type: "string",
      description: "Label for the additional information field",
    },
  },
  required: ["questions"],
};
