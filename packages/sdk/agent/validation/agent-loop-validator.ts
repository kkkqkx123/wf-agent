/**
 * Agent Loop Configuration Validator
 *
 * Provides validation logic for AgentLoopConfigFile using Zod schema from @wf-agent/types.
 * This validator is part of the agent module's validation layer.
 */

import type { AgentLoopConfigFile } from "../../api/shared/config/types.js";
import type { Result, LLMProfile } from "@wf-agent/types";
import {
  ValidationError,
  ConfigurationValidationError,
  AgentLoopDefinitionSchema,
} from "@wf-agent/types";
import { validateToolFormatCompatibility } from "../../services/llm/formatters/tool-format-selector.js";
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
        },
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
    warnings.push(
      `High maxIterations (${config.maxIterations}). Consider if this is intentional to avoid infinite loops.`,
    );
  }

  if (config.hooks && config.hooks.length > 10) {
    warnings.push(`Many hooks configured (${config.hooks.length}). This may impact performance.`);
  }

  return warnings;
}

/**
 * Validation result for tool call protocol compatibility.
 */
export interface ProtocolValidationResult {
  /** Whether the protocol configuration is valid */
  valid: boolean;
  /** Warning messages (non-critical issues) */
  warnings: string[];
  /** Error messages (critical issues) */
  errors: string[];
}

/**
 * Validate tool call protocol compatibility between an agent definition and its profile.
 *
 * Checks:
 * 1. If the definition has `toolCallFormat`, it must be a valid `ToolCallFormatConfig`
 * 2. If the definition has `toolCallFormat` and the profile also has `toolCallFormat`,
 *    they must be compatible
 * 3. Emits warnings on mismatches
 *
 * @param definition The agent loop definition
 * @param profileResolver Function to resolve a profile by ID
 * @returns Validation result with warnings and errors
 */
export function validateAgentToolCallProtocol(
  definition: AgentLoopConfigFile,
  profileResolver: (profileId: string) => LLMProfile | undefined,
): ProtocolValidationResult {
  const warnings: string[] = [];
  const errors: string[] = [];

  const defFormat = definition.toolCallFormat;
  const profileId = definition.profileId || "DEFAULT";
  const profile = profileResolver(profileId);

  // If neither has explicit config, it's valid (defaults to native)
  if (!defFormat && !profile?.toolCallFormat) {
    return { valid: true, warnings, errors };
  }

  if (defFormat && profile?.toolCallFormat) {
    // Both have explicit config — check compatibility
    const compatibility = validateToolFormatCompatibility(
      profile.toolCallFormat.format,
      defFormat.format,
    );
    if (!compatibility.compatible) {
      errors.push(
        `Tool call format mismatch: definition "${definition.id}" specifies "${defFormat.format}" ` +
        `but profile "${profileId}" is configured for "${profile.toolCallFormat.format}". ` +
        `${compatibility.reason || ""}`,
      );
    } else if (compatibility.reason) {
      warnings.push(compatibility.reason);
    }
  } else if (defFormat && !profile?.toolCallFormat) {
    // Definition has explicit config, profile doesn't — this is fine
    // The definition's config will be used at runtime
    warnings.push(
      `Definition "${definition.id}" specifies tool call format "${defFormat.format}" ` +
      `but profile "${profileId}" has no explicit toolCallFormat. ` +
      `The definition's format will be used at runtime.`,
    );
  }

  return {
    valid: errors.length === 0,
    warnings,
    errors,
  };
}
