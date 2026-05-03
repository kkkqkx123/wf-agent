/**
 * Workflow Configuration Processing Functions
 * Provide functions for validating, transforming, and exporting Workflow configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { WorkflowValidator } from "../../../../workflow/validation/workflow-validator.js";
import { ConfigTransformer } from "../config-transformer.js";
import type { WorkflowDefinition } from "@wf-agent/types";
import { WorkflowDefinitionSchema } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";
import { ConfigurationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Verify Workflow configuration
 * Two-phase validation:
 * 1. Lightweight Schema validation (fast, catches format errors)
 * 2. Deep business logic validation (WorkflowValidator)
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateWorkflow(
  config: ParsedConfig<"workflow">,
): Result<ParsedConfig<"workflow">, ValidationError[]> {
  const workflow = config.config as WorkflowDefinition;
  
  // Phase 1: Lightweight Schema validation
  const schemaResult = WorkflowDefinitionSchema.safeParse(workflow);
  if (!schemaResult.success) {
    const errors = schemaResult.error.issues.map((e: any) => new SchemaValidationError(e.message));
    return err(errors);
  }
  
  // Phase 2: Deep business logic validation
  const workflowValidator = new WorkflowValidator();
  const result = workflowValidator.validate(workflow);

  // Use `andThen` for type conversion
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"workflow">, ValidationError[]>;
}

/**
 * Translate Workflow configuration
 * Handle parameter substitution and edge reference updates
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed WorkflowDefinition
 */
export function transformWorkflow(
  config: ParsedConfig<"workflow">,
  parameters?: Record<string, unknown>,
): WorkflowDefinition {
  const transformer = new ConfigTransformer();
  return transformer.transformToWorkflow(config.config, parameters);
}

/**
 * Export Workflow configuration
 * @param workflowDef WorkflowDefinition object
 * @param format Configuration format
 * @returns String containing the configuration file content
 */
export function exportWorkflow(workflowDef: WorkflowDefinition, format: ConfigFormat): string {
  const transformer = new ConfigTransformer();
  const configFile = transformer.transformFromWorkflow(workflowDef);

  switch (format) {
    case "json":
      return stringifyJson(configFile, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support export, please use JSON format",
        format,
        {
          suggestion: "Replace JSON with JSON",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}
