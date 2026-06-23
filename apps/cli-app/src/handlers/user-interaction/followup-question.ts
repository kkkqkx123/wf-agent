/**
 * Follow-up Question Handler for CLI
 * Handles the UI logic for asking follow-up questions in the terminal.
 */

import readline from "readline";
import type { 
  FollowupQuestionRequestData,
  FollowupQuestionResponseData 
} from "@wf-agent/types";

export class CLIFollowupQuestionHandler {
  private rl: readline.Interface;

  constructor() {
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });
  }

  /**
   * Handle the follow-up question request and return structured response
   */
  public async handle(data: FollowupQuestionRequestData): Promise<FollowupQuestionResponseData> {
    console.log("\n" + "=".repeat(60));
    console.log("📝 Follow-up Questions");
    console.log("=".repeat(60) + "\n");

    const answers: FollowupQuestionResponseData["answers"] = [];

    for (const question of data.questions) {
      console.log(`Q${question.index + 1}: ${question.text}`);
      question.options.forEach((opt, idx) => {
        console.log(`  ${idx + 1}. ${opt.value}${opt.description ? ` - ${opt.description}` : ''}`);
      });
      console.log(`  ${question.options.length + 1}. [Custom input]`);
      console.log();

      const answer = await new Promise<string>((resolve) => {
        this.rl.question(`Enter your choice (1-${question.options.length + 1}): `, resolve);
      });

      const choice = parseInt(answer.trim(), 10);
      const isCustom = isNaN(choice) || choice === question.options.length + 1;

      if (isCustom) {
        const customInput = await new Promise<string>((resolve) => {
          this.rl.question("Please enter your custom response: ", resolve);
        });
        answers.push({
          questionIndex: question.index,
          selectedOptionIndex: -1,
          customInput,
          answer: customInput,
        });
      } else {
        const selectedOption = question.options[choice - 1];
        if (selectedOption) {
          answers.push({
            questionIndex: question.index,
            selectedOptionIndex: choice - 1,
            answer: selectedOption.value,
          });
          console.log(`✓ Selected: ${selectedOption.value}\n`);
        }
      }
    }

    let additionalInfo: string | undefined;
    if (data.additionalInfoLabel) {
      console.log(`${data.additionalInfoLabel}:`);
      console.log("(Press Enter to skip)");
      additionalInfo = await new Promise<string>((resolve) => {
        this.rl.question("> ", resolve);
      });
      console.log();
    }

    return {
      answers,
      additionalInfo: additionalInfo?.trim() || undefined,
    };
  }

  /**
   * Cleanup resources
   */
  public cleanup(): void {
    this.rl.close();
  }
}
