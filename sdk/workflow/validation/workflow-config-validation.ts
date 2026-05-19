/**
 * Workflow Configuration Validation Functions
 * 
 * Provides convenient functions for validating workflow configurations.
 * These functions wrap the WorkflowValidator class to provide a simple functional interface.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError, WorkflowTemplateSchema } from "@wf-agent/types";
import { WorkflowValidator } from "./workflow-validator.js";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Validate Workflow configuration
 * Two-phase validation:
 * 1. Lightweight Schema validation (fast, catches format errors)
 * 2. Deep business logic validation (WorkflowValidator)
 * 
 * @param config The workflow configuration object
 * @returns The validation result
 */
export function validateWorkflowConfig(
  config: WorkflowTemplate,
): Result<WorkflowTemplate, ValidationError[]> {
  // Phase 1: Lightweight Schema validation
  const schemaResult = WorkflowTemplateSchema.safeParse(config);
  if (!schemaResult.success) {
    const errors = schemaResult.error.issues.map((e) => 
      new ConfigurationValidationError(e.message, {
        configType: "schema",
        field: e.path.join("."),
      })
    );
    return err(errors);
  }
  
  // Phase 2: Deep business logic validation
  const workflowValidator = new WorkflowValidator();
  const result = workflowValidator.validate(config);

  // Use `andThen` for type conversion
  // ConfigurationValidationError[] is assignable to ValidationError[] (subtype relationship)
  return result.andThen(() => ok(config)) as Result<WorkflowTemplate, ValidationError[]>;
}

/**
 * Get validation warnings for Workflow configuration
 * @param config The workflow configuration object
 * @returns Array of warning messages
 */
export function getWorkflowValidationWarnings(config: WorkflowTemplate): string[] {
  const warnings: string[] = [];

  // Add workflow-specific warnings here
  if (config.nodes && config.nodes.length > 50) {
    warnings.push("Workflow has many nodes (> 50). Consider breaking it into subworkflows for better maintainability.");
  }

  return warnings;
}
