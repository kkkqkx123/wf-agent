/**
 * GenerateBatchCommand - LLM Batch Generation Command
 *
 * Category: Execution
 * Processes multiple LLM requests in parallel
 */

import {
  ExecutionCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
  type CommandMetadataDefinition,
} from "../types/command.js";
import type { LLMRequest, LLMResult } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * LLM batch generation command parameters
 */
export interface GenerateBatchParams {
  /** Array of LLM requests to process */
  requests: LLMRequest[];
}

/**
 * LLM batch generation command
 */
export class GenerateBatchCommand extends ExecutionCommand<LLMResult[]> {
  constructor(
    private readonly params: GenerateBatchParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "GenerateBatchCommand",
      description: "Execute batch LLM text generation for multiple requests",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<LLMResult[]> {
    const llmWrapper = this.dependencies.getLLMWrapper();
    const results = await Promise.all(
      this.params.requests.map(request => llmWrapper.generate(request)),
    );

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

    if (!this.params.requests || this.params.requests.length === 0) {
      errors.push("The request list cannot be empty");
    } else {
      for (let i = 0; i < this.params.requests.length; i++) {
        const request = this.params.requests[i];
        if (!request || !request.messages || request.messages.length === 0) {
          errors.push(`Request ${i} message list cannot be empty`);
        }
      }
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
