/**
 * Trigger template configuration validation function
 * Responsible for verifying the validity of trigger template configurations
 * Note: The actual validation logic is entirely delegated to the trigger-validator function; this serves only as an adapter.
 */

import type { TriggerTemplate } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateWorkflowTrigger } from "../../../../core/validation/trigger-validator.js";
import { validateRequiredFields, validateNumberField } from "./base-validator.js";

/**
 * Verify trigger template configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validateTriggerTemplateConfig(
  config: ConfigFile,
): Result<TriggerTemplate, ValidationError[]> {
  const template = config as TriggerTemplate;
  const errors: ValidationError[] = [];

  // Verify required fields
  errors.push(
    ...validateRequiredFields(
      template as unknown as Record<string, unknown>,
      ["name", "condition", "action", "createdAt", "updatedAt"],
      "TriggerTemplate",
    ),
  );

  // Verify the timestamp
  if (template.createdAt !== undefined) {
    errors.push(
      ...validateNumberField(template.createdAt, "TriggerTemplate.createdAt", {
        integer: true,
        min: 0,
      }),
    );
  }

  if (template.updatedAt !== undefined) {
    errors.push(
      ...validateNumberField(template.updatedAt, "TriggerTemplate.updatedAt", {
        integer: true,
        min: 0,
      }),
    );
  }

  // Trigger configuration validation fully delegated to the core validator
  // Create a temporary WorkflowTrigger object to be used for validation.
  const tempTrigger = {
    id: "temp-trigger-id",
    name: template.name,
    description: template.description,
    condition: template.condition,
    action: template.action,
    enabled: template.enabled,
    maxTriggers: template.maxTriggers,
    metadata: template.metadata,
  };

  const triggerResult = validateWorkflowTrigger(tempTrigger, "TriggerTemplate");
  if (triggerResult.isErr()) {
    errors.push(...triggerResult.error);
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(template);
}
