/**
 * GenerateCommand - LLM Generate Text Command
 *
 * Category: Execution (LLM operations can be long-running)
 * Implements unified parameter pattern for consistency
 */

import {
  ExecutionCommand,
  CommandValidationResult,
  type CommandMetadataDefinition,
} from "../types/command.js";
import { validateGenerateParams } from "./validators/index.js";
import type { LLMRequest, LLMResult } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * LLM generation command parameters
 */
export interface GenerateParams {
  /** LLM request configuration */
  request: LLMRequest;
}

/**
 * LLM generate command
 * Executes LLM text generation with parameter validation
 */
export class GenerateCommand extends ExecutionCommand<LLMResult> {
  constructor(
    private readonly params: GenerateParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "GenerateCommand",
      description: "Execute LLM text generation with messages and options",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<LLMResult> {
    const llmWrapper = this.dependencies.getLLMWrapper();
    const result = await llmWrapper.generate(this.params.request);

    // Handle the Result type, either extract the successful result or throw an error.
    if (result.isErr()) {
      throw result.error;
    }

    return result.value;
  }

  validate(): CommandValidationResult {
    return validateGenerateParams(this.params.request);
  }
}
