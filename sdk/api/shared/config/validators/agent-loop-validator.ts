/**
 * Agent Loop Configuration Validator
 * Provides validation logic for AgentLoopConfigFile using Zod schema from @wf-agent/types
 */

import type { AgentLoopConfigFile } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { AgentLoopDefinitionSchema } from "@wf-agent/types";

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
    const errors = result.error.issues.map((issue: any) => {
      const fieldPath = issue.path.join(".");
      
      // Build base issue details (all ZodIssue variants have these properties)
      const zodIssueDetails: Record<string, unknown> = {
        code: issue.code,
        path: issue.path,
        message: issue.message,
      };
      
      // Type narrowing using Zod's discriminated union
      // Using string literal (ZodIssueCode enum is deprecated in Zod v4)
      if (issue.code === "invalid_type") {
        zodIssueDetails["expected"] = issue.expected;
      }
      
      return new SchemaValidationError(issue.message, {
        field: fieldPath || "unknown",
        value: issue.input, // $ZodIssueBase includes input property
        context: {
          zodIssue: zodIssueDetails,
        },
      });
    });
    return err(errors);
  }

  // Return validated data from Zod (preserves type safety and applies any schema defaults/transforms)
  return ok(result.data);
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
