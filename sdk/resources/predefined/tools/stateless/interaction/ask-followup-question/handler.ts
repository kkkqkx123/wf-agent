/**
 * The logic executed by the ask_followup_question tool
 *
 * This is a special tool that requires user interaction.
 * The handler returns a special result that the workflow engine should handle
 * by presenting the question to the user and waiting for a response.
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Create the `ask_followup_question` tool execution function
 */
export function createAskFollowupQuestionHandler() {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { question, follow_up } = params as {
        question: string;
        follow_up: Array<{ text: string; mode?: string | null }>;
      };

      if (!question || typeof question !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'question' parameter",
        };
      }

      if (!Array.isArray(follow_up) || follow_up.length < 1 || follow_up.length > 4) {
        return {
          success: false,
          content: "",
          error: "follow_up must be an array with 1-4 options",
        };
      }

      // Format the question and options for display
      const optionsText = follow_up
        .map((opt, i) => {
          const modeText = opt.mode ? ` [mode: ${opt.mode}]` : "";
          return `${i + 1}. ${opt.text}${modeText}`;
        })
        .join("\n");

      // Return a special result indicating user interaction is needed
      // The workflow engine should handle this by presenting the question to the user
      return {
        success: true,
        content: `Question: ${question}\n\nOptions:\n${optionsText}`,
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
