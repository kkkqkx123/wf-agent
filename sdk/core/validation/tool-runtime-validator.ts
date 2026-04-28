/**
 * Runtime Validator
 * Responsible for the runtime checking of tool parameters (validation during execution)
 * This includes: parameter values, types, formats, and enum values
 */

import { z } from "zod";
import type { Tool, ToolProperty } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ToolRuntimeValidator" });

/**
 * Runtime Validator
 * Responsible for the runtime checking of tool parameters
 */
export class RuntimeValidator {
  /**
   * Verify tool parameters
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @throws RuntimeValidationError: If parameter validation fails
   */
  validate(tool: Tool, parameters: Record<string, unknown>): void {
    const schema = this.buildSchema(tool);
    const result = schema.safeParse(parameters);

    if (!result.success) {
      const firstError = result.error.issues[0];
      if (!firstError) {
        logger.debug("Tool parameter validation failed", {
          toolId: tool.id,
          reason: "Unknown validation error",
        });
        throw new RuntimeValidationError("Parameter validation failed", {
          operation: "validate",
          field: "parameters",
          value: parameters,
        });
      }
      const field = firstError.path.join(".");
      logger.debug("Tool parameter validation failed", {
        toolId: tool.id,
        field,
        message: firstError.message,
      });
      throw new RuntimeValidationError(firstError.message, {
        operation: "validate",
        field: field,
        value: parameters,
      });
    }
  }

  /**
   * Build a parameter validation schema
   * @param tool: Tool definition
   * @returns: Zod schema
   */
  private buildSchema(tool: Tool): z.ZodType<Record<string, unknown>> {
    const shape: Record<string, z.ZodTypeAny> = {};

    for (const [paramName, paramSchema] of Object.entries(tool.parameters.properties)) {
      const zodSchema = this.buildPropertySchema(paramSchema, tool);

      // Set whether it is mandatory
      if (tool.parameters.required.includes(paramName)) {
        shape[paramName] = zodSchema;
      } else {
        shape[paramName] = zodSchema.optional();
      }
    }

    // Use strict() to enforce additionalProperties: false
    // This aligns with OpenAI strict mode requirements
    return z.object(shape).strict();
  }

  /**
   * Build a property schema recursively
   * @param property: Tool property definition
   * @param tool: Tool definition (for context)
   * @returns: Zod schema
   */
  private buildPropertySchema(property: ToolProperty, tool: Tool): z.ZodTypeAny {
    let zodSchema: z.ZodTypeAny;

    // Build base type schema
    switch (property.type) {
      case "string":
        zodSchema = z.string();
        break;
      case "number":
        zodSchema = z.number();
        break;
      case "integer":
        zodSchema = z.number().int();
        break;
      case "boolean":
        zodSchema = z.boolean();
        break;
      case "array":
        zodSchema = this.buildArraySchema(property, tool);
        break;
      case "object":
        zodSchema = this.buildObjectSchema(property, tool);
        break;
      case "null":
        zodSchema = z.null();
        break;
      default:
        zodSchema = z.any();
    }

    // Add string constraints
    if (property.type === "string") {
      if (property.minLength !== undefined) {
        zodSchema = (zodSchema as z.ZodString).min(property.minLength);
      }
      if (property.maxLength !== undefined) {
        zodSchema = (zodSchema as z.ZodString).max(property.maxLength);
      }
      if (property.pattern !== undefined) {
        zodSchema = (zodSchema as z.ZodString).regex(new RegExp(property.pattern));
      }
    }

    // Add number constraints
    if (property.type === "number" || property.type === "integer") {
      if (property.minimum !== undefined) {
        zodSchema = (zodSchema as z.ZodNumber).min(property.minimum);
      }
      if (property.maximum !== undefined) {
        zodSchema = (zodSchema as z.ZodNumber).max(property.maximum);
      }
    }

    // Add enum validation
    if (property.enum && property.enum.length > 0) {
      const enumValues = property.enum as [string, ...string[]];
      zodSchema = zodSchema.pipe(z.enum(enumValues));
    }

    // Add format validation
    if (property.format && typeof property.format === "string") {
      zodSchema = zodSchema.pipe(this.buildFormatSchema(property.format));
    }

    return zodSchema;
  }

  /**
   * Build an array schema recursively
   * @param property: Tool property definition
   * @param tool: Tool definition (for context)
   * @returns: Zod array schema
   */
  private buildArraySchema(property: ToolProperty, tool: Tool): z.ZodTypeAny {
    if (property.items) {
      const itemSchema = this.buildPropertySchema(property.items, tool);
      return z.array(itemSchema);
    }
    return z.array(z.any());
  }

  /**
   * Build an object schema recursively
   * @param property: Tool property definition
   * @param tool: Tool definition (for context)
   * @returns: Zod object schema
   */
  private buildObjectSchema(property: ToolProperty, tool: Tool): z.ZodTypeAny {
    if (property.properties && Object.keys(property.properties).length > 0) {
      const shape: Record<string, z.ZodTypeAny> = {};
      const required = property.required || [];

      for (const [key, prop] of Object.entries(property.properties)) {
        const propSchema = this.buildPropertySchema(prop, tool);
        if (required.includes(key)) {
          shape[key] = propSchema;
        } else {
          shape[key] = propSchema.optional();
        }
      }

      // Enforce strict mode for nested objects
      return z.object(shape).strict();
    }

    // If no properties defined, use record but this should be avoided in strict mode
    return z.record(z.string(), z.any());
  }

  /**
   * Build a format schema
   * @param format: A format string
   * @returns: A Zod schema
   */
  private buildFormatSchema(format: string): z.ZodTypeAny {
    switch (format) {
      case "uri":
      case "url":
        return z.string().url();
      case "email":
        return z.string().email();
      case "uuid":
        return z.string().uuid();
      case "date-time":
        return z.string().datetime();
      case "date":
        return z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
      case "time":
        return z.string().regex(/^\d{2}:\d{2}:\d{2}$/);
      case "ipv4":
        // IPv4 regex pattern
        return z
          .string()
          .regex(
            /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/,
          );
      case "ipv6":
        // IPv6 regex pattern (simplified)
        return z.string().regex(/^(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$|^::1$|^::$/);
      default:
        // Unknown format, allow any string
        return z.string();
    }
  }
}
