/**
 * The logic executed by the ask_followup_question tool
 *
 * This is a special interactive tool that requires user input to proceed.
 * The handler supports both interactive mode (with event system) and fallback mode (returns formatted text).
 */

import type { ToolOutput, BuiltinToolExecutionContext } from "@wf-agent/types";
import type { EventRegistry } from "../../../../../../core/registry/event-registry.js";
import { buildUserInteractionRequestedEvent } from "../../../../../../core/utils/event/builders/interaction-events.js";
import { generateId, now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Create the `ask_followup_question` tool execution function
 * 
 * This handler supports both interactive mode (with event system) 
 * and fallback mode (returns formatted text).
 */
export function createAskFollowupQuestionHandler() {
  return async (
    params: Record<string, unknown>,
    context: BuiltinToolExecutionContext
  ): Promise<ToolOutput> => {
    const startTime = now();
    
    try {
      const { questions, additionalInfoLabel } = params as {
        questions: Array<{
          text: string;
          options: string[];
        }>;
        additionalInfoLabel?: string;
      };

      // Validate parameters
      if (!questions || !Array.isArray(questions) || questions.length === 0) {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'questions' parameter. Must be a non-empty array.",
        };
      }

      if (questions.length > 3) {
        return {
          success: false,
          content: "",
          error: "Too many questions. Maximum 3 questions allowed per call.",
        };
      }

      // Validate each question
      for (let i = 0; i < questions.length; i++) {
        const q = questions[i];
        if (!q) continue;
        
        if (!q.text || typeof q.text !== "string" || q.text.trim().length === 0) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} must have a non-empty 'text' field.`,
          };
        }
        
        if (!q.options || !Array.isArray(q.options) || q.options.length === 0) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} must have at least 1 option.`,
          };
        }
        
        if (q.options.length > 4) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} has too many options. Maximum 4 options allowed.`,
          };
        }
        
        // Validate each option is a non-empty string
        for (let j = 0; j < q.options.length; j++) {
          const opt = q.options[j];
          if (!opt || typeof opt !== "string" || opt.trim().length === 0) {
            return {
              success: false,
              content: "",
              error: `Option ${j + 1} in question ${i + 1} must be a non-empty string.`,
            };
          }
        }
      }

      // Check if interactive mode is available
      const eventManager = context.eventManager as EventRegistry | undefined;
      if (!eventManager || !context.executionId) {
        // Fallback mode: return formatted text
        return createFallbackResponse(questions, additionalInfoLabel);
      }

      // Interactive mode: trigger user interaction event
      const interactionId = generateId();
      const timeout = 300000; // 5 minutes default

      // Extract nodeId from execution entity if available
      let nodeId: string | undefined;
      if (context.parentExecutionEntity) {
        const entity = context.parentExecutionEntity as { getCurrentNodeId?: () => string };
        nodeId = entity.getCurrentNodeId?.();
      }

      // Build interaction request payload
      const interactionRequest = {
        interactionId,
        operationType: "ASK_FOLLOWUP_QUESTION" as const,
        questions: questions.map((q, idx) => ({
          index: idx,
          text: q.text,
          options: q.options.map((opt, optIdx) => ({
            index: optIdx,
            value: opt,
          })),
        })),
        additionalInfoLabel: additionalInfoLabel || "Additional comments or information",
        metadata: {
          executionId: context.executionId,
          nodeId,
        },
      };

      // Emit USER_INTERACTION_REQUESTED event
      const requestedEvent = buildUserInteractionRequestedEvent({
        executionId: context.executionId,
        interactionId,
        operationType: "ASK_FOLLOWUP_QUESTION",
        prompt: JSON.stringify(interactionRequest),
        timeout,
        nodeId,
      });

      await eventManager.emit(requestedEvent);

      // Wait for USER_INTERACTION_RESPONDED event
      const response = await waitForInteractionResponse(
        eventManager,
        interactionId,
        timeout
      );

      if (!response) {
        return {
          success: false,
          content: "",
          error: "Timeout waiting for user response",
        };
      }

      // Parse user response
      const userAnswers = response.inputData as {
        answers: Array<{
          questionIndex: number;
          selectedOptionIndex: number;
          customInput?: string;
          answer: string;
        }>;
        additionalInfo?: string;
      };

      // Format tool result for LLM
      const formattedResult = formatUserResponse(questions, userAnswers, additionalInfoLabel);

      const executionTime = diffTimestamp(startTime, now());

      return {
        success: true,
        content: formattedResult,
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

/**
 * Create fallback response when interactive mode is unavailable
 */
function createFallbackResponse(
  questions: Array<{ text: string; options: string[] }>,
  additionalInfoLabel?: string
): ToolOutput {
  const lines: string[] = [];
  lines.push("Interactive mode unavailable. Please provide the following information:");
  lines.push("");

  questions.forEach((q, idx) => {
    lines.push(`Q${idx + 1}: ${q.text}`);
    q.options.forEach((opt, optIdx) => {
      lines.push(`  ${optIdx + 1}. ${opt}`);
    });
    lines.push(`  ${q.options.length + 1}. [Custom input]`);
    lines.push("");
  });

  lines.push(`${additionalInfoLabel || "Additional comments"}:`);
  lines.push("[Please provide your responses above]");

  return {
    success: true,
    content: lines.join("\n"),
  };
}

/**
 * Wait for user interaction response
 */
async function waitForInteractionResponse(
  eventManager: EventRegistry,
  interactionId: string,
  timeout: number
): Promise<{ inputData: unknown } | null> {
  return new Promise((resolve) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let resolved = false;

    const listener = (event: any) => {
      if (event.interactionId === interactionId && !resolved) {
        resolved = true;
        if (timeoutId) clearTimeout(timeoutId);
        eventManager.off("USER_INTERACTION_RESPONDED", listener);
        resolve(event);
      }
    };

    eventManager.on("USER_INTERACTION_RESPONDED", listener);

    timeoutId = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        eventManager.off("USER_INTERACTION_RESPONDED", listener);
        resolve(null);
      }
    }, timeout);
  });
}

/**
 * Format user response for LLM consumption
 */
function formatUserResponse(
  questions: Array<{ text: string; options: string[] }>,
  userAnswers: {
    answers: Array<{
      questionIndex: number;
      selectedOptionIndex: number;
      customInput?: string;
      answer: string;
    }>;
    additionalInfo?: string;
  },
  additionalInfoLabel?: string
): string {
  const lines: string[] = [];
  lines.push("User Responses:");
  lines.push("");

  userAnswers.answers.forEach((answer) => {
    const question = questions[answer.questionIndex];
    if (!question) return;

    lines.push(`Q${answer.questionIndex + 1}: ${question.text}`);
    
    if (answer.selectedOptionIndex >= 0) {
      const option = question.options[answer.selectedOptionIndex];
      lines.push(`A${answer.questionIndex + 1}: ${option || answer.answer} (selected from options)`);
    } else {
      lines.push(`A${answer.questionIndex + 1}: ${answer.customInput || answer.answer} (custom input)`);
    }
    lines.push("");
  });

  if (userAnswers.additionalInfo && userAnswers.additionalInfo.trim()) {
    lines.push(`${additionalInfoLabel || "Additional Information"}:`);
    lines.push(userAnswers.additionalInfo);
    lines.push("");
  }

  lines.push("--- End of User Response ---");

  return lines.join("\n");
}
