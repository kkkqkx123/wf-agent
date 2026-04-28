/**
 * Code Module Configuration Validator
 * Provides the validation logic for Code module configurations
 */

// zod is imported for potential future schema validation use
import type { Script, ScriptExecutionOptions, SandboxConfig } from "@wf-agent/types";
import { ScriptType } from "@wf-agent/types";
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

  /**
   * Verify script type compatibility
   * @param scriptType: Script type
   * @param content: Script content
   * @param filePath: File path
   * @throws ValidationError: Throws a ValidationError when the type and content are not compatible
   */
  validateScriptTypeCompatibility(
    scriptType: ScriptType,
    content?: string,
    filePath?: string,
  ): Result<void, ConfigurationValidationError[]> {
    // Verify the compatibility of file extensions with the script type.
    if (filePath) {
      const extension = filePath.toLowerCase().split(".").pop();
      const expectedExtensions = this.getExpectedExtensions(scriptType);

      if (extension && !expectedExtensions.includes(extension)) {
        return err([
          new ConfigurationValidationError(
            `File extension '${extension}' is not compatible with script type '${scriptType}'. Expected: ${expectedExtensions.join(", ")}`,
            {
              configType: "script",
              field: "filePath",
              value: extension,
            },
          ),
        ]);
      }
    }

    // Verify the compatibility of the content with the script type.
    if (content) {
      const contentResult = this.validateContentCompatibility(scriptType, content);
      if (contentResult.isErr()) {
        return contentResult;
      }
    }

    return ok(undefined);
  }

  /**
   * Get the expected file extensions for the script type
   * @param scriptType: The script type
   * @returns: An array of the expected file extensions
   */
  private getExpectedExtensions(scriptType: ScriptType): string[] {
    switch (scriptType) {
      case "SHELL":
        return ["sh", "bash"];
      case "CMD":
        return ["bat", "cmd"];
      case "POWERSHELL":
        return ["ps1"];
      case "PYTHON":
        return ["py"];
      case "JAVASCRIPT":
        return ["js", "ts"];
      default:
        return [];
    }
  }

  /**
   * Verify the compatibility of the content with the script type.
   * @param scriptType: The type of the script
   * @param content: The content of the script
   * @returns: The verification result
   */
  private validateContentCompatibility(
    scriptType: ScriptType,
    content: string,
  ): Result<void, ConfigurationValidationError[]> {
    // Basic syntax check (only records warnings, does not prevent execution)
    switch (scriptType) {
      case "SHELL":
        if (!content.includes("#!/bin/bash") && !content.includes("#!/bin/sh")) {
          logger.validationWarning("Shell script may be missing shebang line", "content", content, {
            configType: "script",
            scriptType,
          });
        }
        break;
      case "POWERSHELL":
        if (!content.includes("#") && !content.includes("Write-Host")) {
          logger.validationWarning(
            "PowerShell script may be missing proper syntax",
            "content",
            content,
            { configType: "script", scriptType },
          );
        }
        break;
      case "PYTHON":
        if (!content.includes("def ") && !content.includes("import ")) {
          logger.validationWarning(
            "Python script may be missing proper syntax",
            "content",
            content,
            { configType: "script", scriptType },
          );
        }
        break;
      case "JAVASCRIPT":
        if (
          !content.includes("function") &&
          !content.includes("const") &&
          !content.includes("let")
        ) {
          logger.validationWarning(
            "JavaScript script may be missing proper syntax",
            "content",
            content,
            { configType: "script", scriptType },
          );
        }
        break;
    }
    return ok(undefined);
  }

  /**
   * Verify the script execution environment
   * @param script: The script definition
   * @param environment: Information about the execution environment
   * @throws ValidationError: Throws a ValidationError if the environment does not meet the requirements
   */
  validateExecutionEnvironment(
    script: Script,
    environment: Record<string, unknown>,
  ): Result<void, ConfigurationValidationError[]> {
    const { type, options } = script;
    const errors: ConfigurationValidationError[] = [];

    // Verify the necessary environment variables.
    if (options.environment) {
      for (const [key, value] of Object.entries(options.environment)) {
        if (typeof value !== "string") {
          errors.push(
            new ConfigurationValidationError(`Environment variable '${key}' must be a string`, {
              configType: "script",
              configPath: "options.environment",
            }),
          );
        }
      }
    }

    // Verify the environment requirements specific to the script type.
    switch (type) {
      case "PYTHON":
        if (!environment["pythonAvailable"]) {
          errors.push(
            new ConfigurationValidationError(
              "Python interpreter is not available in the execution environment",
              {
                configType: "script",
                field: "environment",
              },
            ),
          );
        }
        break;
      case "JAVASCRIPT":
        if (!environment["nodeAvailable"]) {
          errors.push(
            new ConfigurationValidationError(
              "Node.js runtime is not available in the execution environment",
              {
                configType: "script",
                field: "environment",
              },
            ),
          );
        }
        break;
      case "POWERSHELL":
        if (!environment["powershellAvailable"]) {
          errors.push(
            new ConfigurationValidationError(
              "PowerShell is not available in the execution environment",
              {
                configType: "script",
                field: "environment",
              },
            ),
          );
        }
        break;
    }

    if (errors.length === 0) {
      return ok(undefined);
    }
    return err(errors);
  }
}
