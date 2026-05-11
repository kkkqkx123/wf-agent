/**
 * Follow-up Question Handler Implementation
 */

import readline from "readline";
import type { FollowupQuestionRequestedEvent } from "@wf-agent/types";
import type { IInteractionHandler, InteractionResponse } from "../types.js";

export class CLIFollowupQuestionHandler implements IInteractionHandler<FollowupQuestionRequestedEvent> {
  public readonly eventType = "FOLLOWUP_QUESTION_REQUESTED";
  private rl: readline.Interface;

  constructor() {
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });
  }

  public async handle(event: FollowupQuestionRequestedEvent): Promise<InteractionResponse> {
    console.log("\n" + "=".repeat(60));
    console.log("📝 Follow-up Questions");
    console.log("=".repeat(60) + "\n");

    const answers: Array<{
      questionIndex: number;
      selectedOptionIndex: number;
      customInput?: string;
      answer: string;
    }> = [];

    for (const question of event.questions) {
      console.log(`Q${question.index + 1}: ${question.text}`);
      question.options.forEach((opt, idx) => {
        console.log(`  ${idx + 1}. ${opt.value}`);
      });
      console.log(`  ${question.options.length + 1}. [Custom input]`);
      console.log();

      const answer = await this.getUserSelection(question.options.length);
      answer.questionIndex = question.index;
      
      if (answer.selectedOptionIndex >= 0) {
        const selectedOption = question.options[answer.selectedOptionIndex];
        if (selectedOption) {
          answer.answer = selectedOption.value;
          console.log(`✓ Selected: ${selectedOption.value}\n`);
        }
        answers.push(answer);
      } else {
        console.log("Please enter your custom response:");
        const customInput = await this.getUserInput();
        answers.push({ ...answer, customInput, answer: customInput });
        console.log(`✓ Custom input: ${customInput}\n`);
      }
    }

    console.log(`${event.additionalInfoLabel}:`);
    console.log("(Press Enter to skip)");
    const additionalInfo = await this.getUserInput();
    console.log();

    return {
      data: { answers },
      additionalInfo: additionalInfo.trim() || undefined,
    };
  }

  private async getUserSelection(optionCount: number): Promise<{
    questionIndex: number;
    selectedOptionIndex: number;
    answer: string;
  }> {
    return new Promise((resolve) => {
      this.rl.question(`Enter your choice (1-${optionCount + 1}): `, (input) => {
        const choice = parseInt(input.trim(), 10);
        if (isNaN(choice) || choice < 1 || choice > optionCount + 1) {
          console.log("Invalid choice. Please try again.\n");
          resolve(this.getUserSelection(optionCount));
          return;
        }
        resolve({
          questionIndex: -1,
          selectedOptionIndex: choice === optionCount + 1 ? -1 : choice - 1,
          answer: "",
        });
      });
    });
  }

  private async getUserInput(): Promise<string> {
    return new Promise((resolve) => {
      this.rl.question("> ", (input) => resolve(input));
    });
  }

  public cleanup(): void {
    this.rl.close();
  }
}
