/**
 * Batch configuration validation functions
 * Provides batch validation functionality for configurations
 *
 * Design Principles:
 * - All functions are pure functions
 * - Do not hold any state
 * - Collect all errors during batch validation instead of returning on first error
 * - Return error type is ValidationError[][], where each config's errors is an array
 */

import type { WorkflowDefinition } from "@wf-agent/types";
import type { NodeTemplate } from "@wf-agent/types";
import type { TriggerTemplate } from "@wf-agent/types";
import type { Script } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
import { allWithErrors } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateWorkflowConfig } from "./workflow-validator.js";
import { validateNodeTemplateConfig } from "./node-template-validator.js";
import { validateTriggerTemplateConfig } from "./trigger-template-validator.js";
import { validateScriptConfig } from "./script-validator.js";

/**
 * Batch validate workflow configurations
 * @param configs Configuration object array
 * @returns Validation result, returns ValidationError[][] on failure (error array for each config)
 */
export function validateBatchWorkflows(
  configs: ConfigFile[],
): Result<WorkflowDefinition[], ValidationError[][]> {
  const results: Result<WorkflowDefinition, ValidationError[]>[] = configs.map(config =>
    validateWorkflowConfig(config),
  );

  // Collect all errors, rather than just returning the first one.
  return allWithErrors(results);
}

/**
 * Batch validate node template configurations
 * @param configs Configuration object array
 * @returns Validation result, returns ValidationError[][] on failure (error array for each config)
 */
export function validateBatchNodeTemplates(
  configs: ConfigFile[],
): Result<NodeTemplate[], ValidationError[][]> {
  const results: Result<NodeTemplate, ValidationError[]>[] = configs.map(config =>
    validateNodeTemplateConfig(config),
  );

  // Collect all errors, rather than just returning the first one.
  return allWithErrors(results);
}

/**
 * Batch validate trigger template configurations
 * @param configs Configuration object array
 * @returns Validation result, returns ValidationError[][] on failure (error array for each config)
 */
export function validateBatchTriggerTemplates(
  configs: ConfigFile[],
): Result<TriggerTemplate[], ValidationError[][]> {
  const results: Result<TriggerTemplate, ValidationError[]>[] = configs.map(config =>
    validateTriggerTemplateConfig(config),
  );

  // Collect all errors, rather than just returning the first one.
  return allWithErrors(results);
}

/**
 * Batch validate script configurations
 * @param configs Configuration object array
 * @returns Validation result, returns ValidationError[][] on failure (error array for each config)
 */
export function validateBatchScripts(configs: ConfigFile[]): Result<Script[], ValidationError[][]> {
  const results: Result<Script, ValidationError[]>[] = configs.map(config =>
    validateScriptConfig(config),
  );

  // Collect all errors, rather than just returning the first one.
  return allWithErrors(results);
}
