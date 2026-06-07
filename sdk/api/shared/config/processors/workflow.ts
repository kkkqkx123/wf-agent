/**
 * Workflow Configuration Processing Functions
 * Provide functions for validating, transforming, and exporting Workflow configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateWorkflowConfig } from "../../../../workflow/validation/workflow-config-validation.js";
import { ConfigTransformer } from "../utils/config-transformer.js";
import type { WorkflowTemplate } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";

/**
 * Verify Workflow configuration
 * Delegates to the unified config validator in core/validation
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateWorkflow(
  config: ParsedConfig<"workflow">,
): Result<ParsedConfig<"workflow">, ValidationError[]> {
  const workflow = config.config as WorkflowTemplate;
  
  // Delegate to unified config validator
  const result = validateWorkflowConfig(workflow);

  // Use `andThen` for type conversion
  // ConfigurationValidationError[] is assignable to ValidationError[] (subtype relationship)
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"workflow">, ValidationError[]>;
}

/**
 * Translate Workflow configuration
 * Handle parameter substitution and edge reference updates
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed WorkflowTemplate
 */
export function transformWorkflow(
  config: ParsedConfig<"workflow">,
  parameters?: Record<string, unknown>,
): WorkflowTemplate {
  const transformer = new ConfigTransformer();
  return transformer.transformToWorkflow(config.config, parameters);
}

/**
 * Export Workflow configuration
 * Returns typed data ready for serialization.
 * @param workflowDef WorkflowTemplate object
 * @returns The workflow data ready for export
 */
export function exportWorkflow(workflowDef: WorkflowTemplate): WorkflowTemplate {
  return workflowDef;
}
