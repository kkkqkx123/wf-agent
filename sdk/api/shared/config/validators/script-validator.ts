/**
 * Script configuration validation function
 * Responsible for verifying the validity of script configurations
 * Note: The actual validation logic is entirely delegated to CodeConfigValidator; this function serves only as an adapter.
 */

import type { Script } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { CodeConfigValidator } from "../../../../workflow/validation/script-config-validator.js";
import { validateRequiredFields, validateBooleanField } from "./base-validator.js";

/**
 * Verify script configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validateScriptConfig(config: ConfigFile): Result<Script, ValidationError[]> {
  const script = config as Script;
  const errors: ValidationError[] = [];
  const codeConfigValidator = new CodeConfigValidator();

  // Verify required fields
  errors.push(
    ...validateRequiredFields(
      script as unknown as Record<string, unknown>,
      ["id", "name", "type", "description", "options"],
      "Script",
    ),
  );

  // Verify the activation status.
  if (script.enabled !== undefined) {
    errors.push(...validateBooleanField(script.enabled, "Script.enabled"));
  }

  // Fully delegate the script configuration validation to CodeConfigValidator.
  const scriptResult = codeConfigValidator.validateScript(script);
  if (scriptResult.isErr()) {
    errors.push(...scriptResult.error);
  }

  // Verify script type compatibility
  if (script.type && (script.content || script.filePath)) {
    const compatibilityResult = codeConfigValidator.validateScriptTypeCompatibility(
      script.type,
      script.content,
      script.filePath,
    );
    if (compatibilityResult.isErr()) {
      errors.push(...compatibilityResult.error);
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(script);
}
