/**
 * GenerateCommand - LLM generates commands
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../types/command.js";
import type { LLMRequest, LLMResult } from "@wf-agent/types";
import { APIDependencyManager } from "../core/sdk-dependencies.js";

/**
 * LLM generation command
 */
export class GenerateCommand extends BaseCommand<LLMResult> {
  constructor(
    private readonly request: LLMRequest,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<LLMResult> {
    const llmWrapper = this.dependencies.getLLMWrapper();
    const result = await llmWrapper.generate(this.request);

    // Handle the Result type, either extract the successful result or throw an error.
    if (result.isErr()) {
      throw result.error;
    }

    return result.value;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.request.messages || this.request.messages.length === 0) {
      errors.push("The message list cannot be empty.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
