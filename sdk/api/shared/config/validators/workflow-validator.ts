/**
 * Workflow configuration validation function
 * Responsible for verifying the validity of the workflow configuration
 * Note: The actual validation logic is delegated to WorkflowValidator; this function serves only as an adapter.
 */

import type { WorkflowDefinition } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { WorkflowValidator } from "../../../../graph/validation/workflow-validator.js";

/**
 * Verify workflow configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validateWorkflowConfig(
  config: ConfigFile,
): Result<WorkflowDefinition, ValidationError[]> {
  const workflow = config as WorkflowDefinition;
  const workflowValidator = new WorkflowValidator();

  // Delegate the validation to WorkflowValidator.
  return workflowValidator.validate(workflow);
}
