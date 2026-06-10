/**
 * Agent Loop Configuration Validator
 * 
 * Provides validation logic for AgentLoopConfigFile using Zod schema from @wf-agent/types.
 * This validator is part of the agent module's validation layer.
 */

import type { AgentLoopConfigFile } from "../../api/shared/config/types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError, AgentLoopDefinitionSchema } from "@wf-agent/types";
import type { ZodIssue } from "zod";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Validate Agent Loop configuration using Zod schema
 * @param config The configuration object
 * @returns Validation result
 */
export function validateAgentLoopConfig(
  config: AgentLoopConfigFile,
): Result<AgentLoopConfigFile, ValidationError[]> {
  const result = AgentLoopDefinitionSchema.safeParse(config);

  if (!result.success) {
    const errors = result.error.issues.map((issue: ZodIssue) => {
      const fieldPath = issue.path.join(".");
      return new ConfigurationValidationError(
        `${fieldPath ? fieldPath + ": " : ""}${issue.message}`,
        {
          configType: "schema",
          field: fieldPath || undefined,
        }
      );
    });
    return err(errors);
  }

  return ok(config);
}

/**
 * Get validation warnings for Agent Loop configuration
 * @param config The configuration object
 * @returns Array of warning messages
 */
export function getAgentLoopValidationWarnings(config: AgentLoopConfigFile): string[] {
  const warnings: string[] = [];

  // Add agent loop-specific warnings here
  if (config.maxIterations && config.maxIterations > 100) {
    warnings.push(`High maxIterations (${config.maxIterations}). Consider if this is intentional to avoid infinite loops.`);
  }

  if (config.hooks && config.hooks.length > 10) {
    warnings.push(`Many hooks configured (${config.hooks.length}). This may impact performance.`);
  }

  return warnings;
}
