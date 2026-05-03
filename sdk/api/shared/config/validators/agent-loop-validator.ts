/**
 * Agent Loop Configuration Validator
 * Provides validation logic for AgentLoopConfigFile using Zod schema
 */

import type { AgentLoopConfigFile } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { AgentLoopConfigFileSchema } from "./agent-loop-schema.js";

/**
 * Validate Agent Loop configuration using Zod schema
 * @param config The configuration object
 * @returns Validation result
 */
export function validateAgentLoopConfig(
  config: AgentLoopConfigFile,
): Result<AgentLoopConfigFile, ValidationError[]> {
  const result = AgentLoopConfigFileSchema.safeParse(config);

  if (!result.success) {
    const errors = result.error.issues.map((issue: any) => {
      const fieldPath = issue.path.join(".");
      return new SchemaValidationError(issue.message, {
        field: fieldPath || "unknown",
        context: {
          code: "SCHEMA_VALIDATION_ERROR",
          expected: issue.expected,
          received: issue.received,
        },
      });
    });
    return err(errors);
  }

  // Return original config (not result.data) to preserve exact type
  return ok(config);
}

/**
 * Get validation warnings for Agent Loop configuration
 * @param config The configuration object
 * @returns Array of warning messages
 */
export function getAgentLoopValidationWarnings(config: AgentLoopConfigFile): string[] {
  const warnings: string[] = [];

  if (config.maxIterations && config.maxIterations > 100) {
    warnings.push("maxIterations is unusually high (> 100). Consider reducing it to avoid excessive costs.");
  }

  return warnings;
}
