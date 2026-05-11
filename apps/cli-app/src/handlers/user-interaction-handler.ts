/**
 * CLI User Interaction Handler
 * Handles interactive tools like ask_followup_question in CLI environment
 */

import readline from "readline";
import type { 
  FollowupQuestionRequestedEvent,
  FollowupQuestionRespondedEvent,
  FollowupQuestionFailedEvent
} from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import type { SDKInstance } from "@wf-agent/sdk";

/**
 * Ask Follow-up Question Request Data
 */
interface AskFollowupQuestionRequest {
  interactionId: string;
  operationType: "ASK_FOLLOWUP_QUESTION";
  questions: Array<{
    index: number;
    text: string;
    options: Array<{
      index: number;
      value: string;
    }>;
  }>;
  additionalInfoLabel: string;
  metadata?: {
    executionId?: string;
    nodeId?: string;
  };
}

/**
 * CLI User Interaction Handler
 */
export class CLIUserInteractionHandler {
  private rl: readline.Interface;
  private sdkInstance: SDKInstance | null = null;

  constructor() {
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });
  }

  /**
   * Initialize the handler with SDK instance
   */
  initialize(sdkInstance: SDKInstance): void {
    this.sdkInstance = sdkInstance;
    
    // Get event manager from SDK's global context
    const globalContext = (sdkInstance as any).globalContext;
    if (!globalContext) {
      console.error("Failed to access SDK global context");
      return;
    }
    
    const eventManager = globalContext.eventRegistry;
    
    // Subscribe to FOLLOWUP_QUESTION_REQUESTED events
    eventManager.on("FOLLOWUP_QUESTION_REQUESTED", async (event: FollowupQuestionRequestedEvent) => {
      await this.handleAskFollowupQuestion(event);
    });
  }

  /**
   * Handle ASK_FOLLOWUP_QUESTION interaction
   */
  private async handleAskFollowupQuestion(event: FollowupQuestionRequestedEvent): Promise<void> {
    const timeoutMs = event.timeout || 300000; // Default 5 minutes
    let timeoutId: NodeJS.Timeout | null = null;
    let isCompleted = false;

    try {
      console.log("\n" + "=".repeat(60));
      console.log("📝 Follow-up Questions");
      console.log("=".repeat(60) + "\n");

      // Set up timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => {
          if (!isCompleted) {
            reject(new Error(`User interaction timeout after ${timeoutMs}ms`));
          }
        }, timeoutMs);
      });

      // Race between user input and timeout
      await Promise.race([
        this.collectAnswers(event),
        timeoutPromise,
      ]).then(async (answers) => {
        isCompleted = true;
        if (timeoutId) clearTimeout(timeoutId);
        
        // Get additional information
        console.log(`${event.additionalInfoLabel}:`);
        console.log("(Press Enter to skip)");
        const additionalInfo = await this.getUserInput();
        console.log();

        // Build response
        const responseData: FollowupQuestionRespondedEvent = {
          id: `followup-response-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
          type: "FOLLOWUP_QUESTION_RESPONDED",
          timestamp: now(),
          executionId: event.metadata?.executionId,
          interactionId: event.interactionId,
          answers,
          additionalInfo: additionalInfo.trim() || undefined,
        };

        // Emit FOLLOWUP_QUESTION_RESPONDED event
        const globalContext = (this.sdkInstance as any)?.globalContext;
        if (globalContext) {
          globalContext.eventRegistry.emit(responseData.type, responseData);
        }

        console.log("✅ Response submitted successfully\n");
      });

    } catch (error) {
      if (timeoutId) clearTimeout(timeoutId);
      
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      // Check if it's a timeout
      if (errorMessage.includes("timeout")) {
        console.error("⏱️  Interaction timed out");
      } else {
        console.error("❌ Error handling interaction:", errorMessage);
      }
      
      // Emit failure event
      const globalContext = (this.sdkInstance as any)?.globalContext;
      if (globalContext) {
        const failedEvent: FollowupQuestionFailedEvent = {
          id: `followup-failed-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
          type: "FOLLOWUP_QUESTION_FAILED",
          timestamp: now(),
          executionId: event.metadata?.executionId,
          interactionId: event.interactionId,
          error: errorMessage,
        };
        globalContext.eventRegistry.emit(failedEvent.type, failedEvent);
      }
    }
  }

  /**
   * Collect answers for all questions
   */
  private async collectAnswers(event: FollowupQuestionRequestedEvent): Promise<Array<{
    questionIndex: number;
    selectedOptionIndex: number;
    customInput?: string;
    answer: string;
  }>> {
    const answers: Array<{
      questionIndex: number;
      selectedOptionIndex: number;
      customInput?: string;
      answer: string;
    }> = [];

    // Process each question
    for (const question of event.questions) {
      console.log(`Q${question.index + 1}: ${question.text}`);
      
      // Display options
      question.options.forEach((opt, idx) => {
        console.log(`  ${idx + 1}. ${opt.value}`);
      });
      console.log(`  ${question.options.length + 1}. [Custom input]`);
      console.log();

      // Get user selection
      const answer = await this.getUserSelection(question.options.length);
      // Set the correct question index
      answer.questionIndex = question.index;
      
      if (answer.selectedOptionIndex >= 0) {
        // Preset option selected
        const selectedOption = question.options[answer.selectedOptionIndex];
        if (selectedOption) {
          answer.answer = selectedOption.value;
          console.log(`✓ Selected: ${selectedOption.value}\n`);
        }
        answers.push(answer);
      } else {
        // Custom input
        console.log("Please enter your custom response:");
        const customInput = await this.getUserInput();
        const answerWithCustom: typeof answer & { customInput?: string } = {
          ...answer,
          customInput,
          answer: customInput,
        };
        console.log(`✓ Custom input: ${customInput}\n`);
        answers.push(answerWithCustom);
      }
    }

    return answers;
  }

  /**
   * Get user selection from numbered options
   */
  private async getUserSelection(optionCount: number): Promise<{
    questionIndex: number;
    selectedOptionIndex: number;
    answer: string;
  }> {
    return new Promise((resolve) => {
      const promptText = `Enter your choice (1-${optionCount + 1}): `;
      
      this.rl.question(promptText, (input) => {
        const choice = parseInt(input.trim(), 10);
        
        if (isNaN(choice) || choice < 1 || choice > optionCount + 1) {
          console.log("Invalid choice. Please try again.\n");
          resolve(this.getUserSelection(optionCount));
          return;
        }

        if (choice === optionCount + 1) {
          // Custom input
          resolve({
            questionIndex: -1, // Will be set by caller
            selectedOptionIndex: -1,
            answer: "",
          });
        } else {
          // Preset option
          resolve({
            questionIndex: -1, // Will be set by caller
            selectedOptionIndex: choice - 1,
            answer: "", // Will be set by caller
          });
        }
      });
    });
  }

  /**
   * Get free-form user input
   */
  private async getUserInput(): Promise<string> {
    return new Promise((resolve) => {
      this.rl.question("> ", (input) => {
        resolve(input);
      });
    });
  }

  /**
   * Cleanup resources
   */
  cleanup(): void {
    this.rl.close();
  }
}
