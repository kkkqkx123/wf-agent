/**
 * Code Module Configuration Validator
 * Provides the validation logic for Code module configurations
 */

// zod is imported for potential future schema validation use
import type { Script, ScriptExecutionOptions, SandboxConfig } from "@wf-agent/types";
import { ScriptSchema, SandboxConfigSchema, ScriptExecutionOptionsSchema } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { validateConfig } from "../../core/validation/utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Code Configuration Validator Class
 */
export class CodeConfigValidator {
  /**
   * Verify script definition
   * @param script The script definition
   * @throws ValidationError Throws a ValidationError when the script definition is invalid
   */
  validateScript(script: Script): Result<Script, ConfigurationValidationError[]> {
    return validateConfig(script, ScriptSchema, "script", "script");
  }

  /**
   * Verify script execution options
   * @param options Script execution options
   * @throws ValidationError Throws a ValidationError when the execution options are invalid
   */
  validateExecutionOptions(
    options: ScriptExecutionOptions,
  ): Result<ScriptExecutionOptions, ConfigurationValidationError[]> {
    return validateConfig(options, ScriptExecutionOptionsSchema, "options", "script");
  }

  /**
   * Verify sandbox configuration
   * @param config Sandbox configuration
   * @throws ValidationError Throws a ValidationError if the sandbox configuration is invalid
   */
  validateSandboxConfig(
    config: SandboxConfig,
  ): Result<SandboxConfig, ConfigurationValidationError[]> {
    return validateConfig(config, SandboxConfigSchema, "sandbox", "script");
  }
}
