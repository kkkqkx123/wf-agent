/**
 * Agent Loop Configuration Validator
 *
 * Validates the validity of the AgentLoopConfigFile
 */

import type { AgentLoopConfigFile, AgentHookConfigFile } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Verification warnings (used to indicate issues that have been resolved but still require attention)
 */
export interface AgentLoopValidationWarnings {
  warnings: string[];
}

/**
 * Verify Agent Loop configuration
 * @param config Agent Loop configuration
 * @returns Verification result: returns the configuration object if successful, or an array of ValidationError if failed
 */
export function validateAgentLoopConfig(
  config: AgentLoopConfigFile,
): Result<AgentLoopConfigFile, ValidationError[]> {
  const errors: ValidationError[] = [];

  // Verify required fields
  if (!config.id) {
    errors.push(
      new SchemaValidationError("The Agent Loop configuration must include an id field.", {
        field: "id",
        value: config.id,
      }),
    );
  }

  // Verify profileId
  if (config.profileId && typeof config.profileId !== "string") {
    errors.push(
      new SchemaValidationError("`profileId` must be a string.", {
        field: "profileId",
        value: config.profileId,
      }),
    );
  }

  // Verify maxIterations
  if (config.maxIterations !== undefined) {
    if (typeof config.maxIterations !== "number") {
      errors.push(
        new SchemaValidationError("`maxIterations` must be a number.", {
          field: "maxIterations",
          value: config.maxIterations,
        }),
      );
    } else if (config.maxIterations < -1 || config.maxIterations === 0) {
      errors.push(
        new SchemaValidationError("`maxIterations` must be a positive integer or -1 (unlimited).", {
          field: "maxIterations",
          value: config.maxIterations,
        }),
      );
    }
  }

  // Validation tools
  if (config.tools) {
    if (!Array.isArray(config.tools)) {
      errors.push(
        new SchemaValidationError("`tools` must be an array.", {
          field: "tools",
          value: config.tools,
        }),
      );
    } else {
      config.tools.forEach((tool, index) => {
        if (typeof tool !== "string") {
          errors.push(
            new SchemaValidationError(`tools[${index}] must be a string`, {
              field: `tools[${index}]`,
              value: tool,
            }),
          );
        }
      });
    }
  }

  // Verify hooks
  if (config.hooks) {
    if (!Array.isArray(config.hooks)) {
      errors.push(
        new SchemaValidationError("Hooks must be an array.", {
          field: "hooks",
          value: config.hooks,
        }),
      );
    } else {
      config.hooks.forEach((hook, index) => {
        const hookErrors = validateHook(hook, index);
        errors.push(...hookErrors);
      });
    }
  }

  // Verify triggers (for future expansion)
  if (config.triggers) {
    if (!Array.isArray(config.triggers)) {
      errors.push(
        new SchemaValidationError("The triggers must be an array.", {
          field: "triggers",
          value: config.triggers,
        }),
      );
    }
    // Note: The "triggers" warnings are processed by the caller.
  }

  // Verify that systemPrompt and systemPromptTemplate are mutually exclusive (handle this as a warning).
  // Note: The exclusion warning should be handled by the caller.

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(config);
}

/**
 * Get validation warnings (check after validation is passed)
 * @param config Agent Loop configuration
 * @returns Array of warnings
 */
export function getAgentLoopValidationWarnings(config: AgentLoopConfigFile): string[] {
  const warnings: string[] = [];

  // Verify triggers (for future expansion)
  if (config.triggers && Array.isArray(config.triggers)) {
    warnings.push(
      "The triggers configuration is defined, but the current Agent Loop layer does not directly support the Trigger function.",
    );
  }

  // Verify that systemPrompt and systemPromptTemplate are mutually exclusive.
  if (config.systemPrompt && config.systemPromptTemplate) {
    warnings.push(
      "Both `systemPrompt` and `systemPromptTemplate` have been defined, with `systemPrompt` being used preferentially.",
    );
  }

  return warnings;
}

/**
 * Verify a single Hook configuration
 * @param hook Hook configuration
 * @param index Hook index
 * @returns List of verification errors
 */
function validateHook(hook: AgentHookConfigFile, index: number): ValidationError[] {
  const errors: ValidationError[] = [];
  const prefix = `hooks[${index}]`;

  // Verify hookType
  const validHookTypes = [
    "BEFORE_ITERATION",
    "AFTER_ITERATION",
    "BEFORE_TOOL_CALL",
    "AFTER_TOOL_CALL",
    "BEFORE_LLM_CALL",
    "AFTER_LLM_CALL",
  ];

  if (!hook.hookType) {
    errors.push(
      new SchemaValidationError(`${prefix} must contain hookType field`, {
        field: `${prefix}.hookType`,
        value: hook.hookType,
      }),
    );
  } else if (!validHookTypes.includes(hook.hookType)) {
    errors.push(
      new SchemaValidationError(
        `${prefix}.hookType must be a valid Hook type: ${validHookTypes.join(", ")}`,
        { field: `${prefix}.hookType`, value: hook.hookType },
      ),
    );
  }

  // Verify eventName
  if (!hook.eventName) {
    errors.push(
      new SchemaValidationError(`${prefix} must contain eventName field`, {
        field: `${prefix}.eventName`,
        value: hook.eventName,
      }),
    );
  } else if (typeof hook.eventName !== "string") {
    errors.push(
      new SchemaValidationError(`${prefix}.eventName must be a string`, {
        field: `${prefix}.eventName`,
        value: hook.eventName,
      }),
    );
  }

  // Verify weight
  if (hook.weight !== undefined && typeof hook.weight !== "number") {
    errors.push(
      new SchemaValidationError(`${prefix}.weight must be a number`, {
        field: `${prefix}.weight`,
        value: hook.weight,
      }),
    );
  }

  // Verify that it is enabled.
  if (hook.enabled !== undefined && typeof hook.enabled !== "boolean") {
    errors.push(
      new SchemaValidationError(`${prefix}.enabled must be a boolean`, {
        field: `${prefix}.enabled`,
        value: hook.enabled,
      }),
    );
  }

  return errors;
}
