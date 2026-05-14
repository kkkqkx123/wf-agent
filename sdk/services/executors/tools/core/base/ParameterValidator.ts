/**
 * Parameter Validator
 * Responsible for verifying whether tool parameters conform to the defined schema
 */

import { z } from "zod";
import type { Tool } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

/**
 * Parameter Validator
 */
export class ParameterValidator {
  /**
   * Verify tool parameters
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @throws ValidationError: If parameter validation fails
   */
  validate(tool: Tool, parameters: Record<string, unknown>): void {
    const schema = this.buildSchema(tool);
    const result = schema.safeParse(parameters);

    if (!result.success) {
      const firstError = result.error.issues[0];
      if (!firstError) {
        throw new RuntimeValidationError("Parameter validation failed", {
          operation: "validate",
          field: "parameters",
          value: parameters,
        });
      }
      const field = firstError.path.join(".");
      throw new RuntimeValidationError(firstError.message, {
        operation: "validate",
        field: field,
        value: parameters,
      });
    }
  }

  /**
   * Construct a parameter validation schema
   * @param tool: Tool definition
   * @returns: Zod schema
   */
  private buildSchema(tool: Tool): z.ZodType<Record<string, unknown>> {
    const shape: Record<string, z.ZodTypeAny> = {};

    for (const [paramName, paramSchema] of Object.entries(tool.parameters.properties)) {
      let zodSchema = this.buildTypeSchema(paramSchema.type);

      // Add enumeration validation
      if (paramSchema.enum && paramSchema.enum.length > 0) {
        zodSchema = zodSchema.pipe(z.enum(paramSchema.enum as [string, ...string[]]));
      }

      // Add format validation
      if (paramSchema.format && typeof paramSchema.format === "string") {
        zodSchema = zodSchema.pipe(this.buildFormatSchema(paramSchema.format));
      }

      // Set whether it is mandatory
      if (tool.parameters.required.includes(paramName)) {
        shape[paramName] = zodSchema;
      } else {
        shape[paramName] = zodSchema.optional();
      }
    }

    return z.object(shape);
  }

  /**
   * Construct a type schema
   * @param type: A string representing the type
   * @returns: A Zod schema
   */
  private buildTypeSchema(type: string): z.ZodTypeAny {
    switch (type) {
      case "string":
        return z.string();
      case "number":
        return z.number();
      case "boolean":
        return z.boolean();
      case "array":
        return z.array(z.unknown());
      case "object":
        return z.record(z.string(), z.unknown());
      default:
        return z.unknown();
    }
  }

  /**
   * Build a format schema
   * @param format: The format string
   * @returns: A Zod schema
   */
  private buildFormatSchema(format: string): z.ZodTypeAny {
    switch (format) {
      case "uri":
        return z.string().url();
      case "email":
        return z.string().email();
      case "uuid":
        return z.string().uuid();
      case "date-time":
        return z.string().datetime();
      default:
        return z.unknown();
    }
  }
}
