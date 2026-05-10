/**
 * Tool Description for `ask_followup_question`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "ask_followup_question",
  type: "STATELESS",
  category: "interaction",
  description: `Ask the user 1-3 questions to gather information needed to complete the task.

Use this tool when you need clarification, user preferences, or decisions before proceeding. Each question should include 1-4 relevant preset options. An additional information field is automatically included for free-form feedback.

Best Practices:
- Ask clear, specific questions (max 3)
- Provide 1-4 relevant preset options FOR EACH QUESTION
- Keep options directly related to the question
- Use additional info field for open-ended feedback`,
  parameters: [
    {
      name: "questions",
      type: "array",
      required: true,
      description: "Array of 1-3 questions, each with 1-4 preset options",
    },
    {
      name: "additionalInfoLabel",
      type: "string",
      required: false,
      description: "Custom label for the additional information field",
    },
  ],
  tips: [
    "Limit to 3 questions maximum to avoid overwhelming users",
    "Each question should have its own relevant options",
    "Keep questions concise and focused",
    "Options should be directly related to their question",
  ],
};
