/**
 * Tool Description for `ask_followup_question`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "ask_followup_question",
  id: "ask_followup_question",
  type: "STATELESS",
  category: "code",
  description: `Ask the user a question to gather additional information needed to complete the task. Use when you need clarification or more details to proceed effectively.

This tool enables interactive problem-solving by allowing direct communication with the user. Use judiciously to maintain a balance between gathering necessary information and avoiding excessive back-and-forth.`,
  parameters: [
    {
      name: "question",
      type: "string",
      required: true,
      description: "A clear, specific question addressing the information needed",
    },
    {
      name: "follow_up",
      type: "array",
      required: true,
      description:
        "A list of 2-4 suggested answers. Suggestions must be complete, actionable answers without placeholders. Optionally include mode to switch modes (code/architect/etc.)",
    },
  ],
  tips: [
    "Use when you need clarification or more details to proceed",
    "Provide 2-4 suggested answers that are complete and actionable",
    "Can optionally include mode switch in suggestions",
  ],
};
