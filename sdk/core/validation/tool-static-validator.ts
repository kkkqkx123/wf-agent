/**
 * Static Validator
 * Responsible for static checks on tool configuration (validation during registration)
 * This includes: tool definitions, parameter schemas, and configuration structures
 */

import { z } from "zod";
import type { Tool, ToolParameterSchema } from "@wf-agent/types";
import { ToolType } from "@wf-agent/types";
import {
  ToolParametersSchema,
  StatelessToolConfigSchema,
  StatefulToolConfigSchema,
  RestToolConfigSchema,
  ToolDefinitionSchema,
} from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateConfig } from "./utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ToolStaticValidator" });

/**
 * Static Validator
 * Responsible for the static checking of tool configurations
 */
export class StaticValidator {
  /**
   * Validation tool definition
   * @param tool: Tool definition
   * @returns: Validation result
   */
  validateTool(tool: Tool): Result<Tool, ConfigurationValidationError[]> {
    const result = validateConfig(tool, ToolDefinitionSchema, "tool", "tool");

    if (result.isErr()) {
      logger.debug("Tool validation failed", {
        toolId: tool.id,
        toolType: tool.type,
        errors: result.error.map(e => e.message),
      });
    }

    // Type assertion to handle Zod schema type inference
    return result as Result<Tool, ConfigurationValidationError[]>;
  }

  /**
   * Verify tool parameter schema
   * @param parameters: The tool parameter schema
   * @returns: The verification result
   */
  validateParameters(
    parameters: ToolParameterSchema,
  ): Result<ToolParameterSchema, ConfigurationValidationError[]> {
    const result = validateConfig(parameters, ToolParametersSchema, "parameters", "tool");
    // Type assertion to handle Zod schema type inference
    return result as Result<ToolParameterSchema, ConfigurationValidationError[]>;
  }

  /**
   * Verify tool configuration
   * @param toolType: Tool type
   * @param config: Tool configuration
   * @returns: Verification result
   */
  validateToolConfig(
    toolType: ToolType,
    config: unknown,
  ): Result<unknown, ConfigurationValidationError[]> {
    let result: z.ZodSafeParseResult<unknown>;

    switch (toolType) {
      case "STATELESS":
        result = StatelessToolConfigSchema.safeParse(config);
        break;
      case "STATEFUL":
        result = StatefulToolConfigSchema.safeParse(config);
        break;
      case "REST":
        result = RestToolConfigSchema.safeParse(config);
        break;
      default:
        return err([
          new ConfigurationValidationError(`Unknown tool type: ${toolType}`, {
            configType: "tool",
            field: "type",
          }),
        ]);
    }

    if (!result.success) {
      const errors = result.error.issues.map(
        issue =>
          new ConfigurationValidationError(issue.message, {
            configType: "tool",
            configPath: `config.${issue.path.map(p => String(p)).join(".")}`,
          }),
      );
      return err(errors);
    }
    return ok(result.data);
  }
}
