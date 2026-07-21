/**
 * Hook Template Validator
 *
 * Shared validation logic for hook template validation across Agent and Workflow domains.
 * Extracted from duplicated implementations in AgentHookTemplateRegistryAPI and HookTemplateRegistryAPI.
 */

import type { HookTemplate } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Validate a hook template (no side effects).
 *
 * @param template - Hook template to validate
 * @returns Validation result
 */
export function validateHookTemplate(
  template: HookTemplate,
): Result<HookTemplate, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (!template.name || typeof template.name !== "string") {
    errors.push(
      new ConfigurationValidationError("Hook template name is required and must be a string", {
        configType: "hook_template",
        configPath: "template.name",
        field: "name",
      }),
    );
  }

  if (!template.hook) {
    errors.push(
      new ConfigurationValidationError("Hook template hook configuration is required", {
        configType: "hook_template",
        configPath: "template.hook",
        field: "hook",
      }),
    );
  } else {
    if (!template.hook.hookType) {
      errors.push(
        new ConfigurationValidationError("Hook template hook.hookType is required", {
          configType: "hook_template",
          configPath: "template.hook.hookType",
          field: "hookType",
        }),
      );
    }
    if (!template.hook.eventName) {
      errors.push(
        new ConfigurationValidationError("Hook template hook.eventName is required", {
          configType: "hook_template",
          configPath: "template.hook.eventName",
          field: "eventName",
        }),
      );
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(template);
}