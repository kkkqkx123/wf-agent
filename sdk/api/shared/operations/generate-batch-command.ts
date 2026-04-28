/**
 * GenerateBatchCommand - LLM batch generation command
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
 * LLM batch generation commands
 */
export class GenerateBatchCommand extends BaseCommand<LLMResult[]> {
  constructor(
    private readonly requests: LLMRequest[],
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<LLMResult[]> {
    const llmWrapper = this.dependencies.getLLMWrapper();
    const results = await Promise.all(this.requests.map(request => llmWrapper.generate(request)));

    // Handle the Result type, either extracting the successful result or throwing an error.
    const llmResults: LLMResult[] = [];
    for (const result of results) {
      if (result.isErr()) {
        throw result.error;
      }
      llmResults.push(result.value);
    }

    return llmResults;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.requests || this.requests.length === 0) {
      errors.push("The request list cannot be empty");
    }

    for (let i = 0; i < this.requests.length; i++) {
      const request = this.requests[i];
      if (!request || !request.messages || request.messages.length === 0) {
        errors.push(`Request ${i} message list cannot be empty`);
      }
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
