/**
 * Zod Schemas for Tool Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import type { ToolType } from "./state.js";

// ============================================================================
// Tool Parameter Property Schema (JSON Schema Draft 2020-12)
// ============================================================================

/**
 * Tool Parameter Property Schema
 */
export const ToolPropertySchema: z.ZodType<unknown> = z.lazy(
  () =>
    z
      .object({
        type: z.enum(["string", "number", "integer", "boolean", "array", "object", "null"]),
        description: z.string().optional(),
        default: z.unknown().optional(),
        enum: z.array(z.unknown()).optional(),
        format: z.string().optional(),
        minLength: z.number().nonnegative().optional(),
        maxLength: z.number().nonnegative().optional(),
        pattern: z.string().optional(),
        minimum: z.number().optional(),
        maximum: z.number().optional(),
        properties: z.record(z.string(), ToolPropertySchema).optional(),
        required: z.array(z.string()).optional(),
        additionalProperties: z.union([z.boolean(), ToolPropertySchema]).optional(),
        items: ToolPropertySchema.optional(),
      })
      .passthrough(), // Allow additional properties per JSON Schema spec
);

// ============================================================================
// Tool Parameters Schema
// ============================================================================

/**
 * Tool Parameters Schema
 */
export const ToolParametersSchema = z
  .object({
    type: z.literal("object"),
    properties: z.record(z.string(), ToolPropertySchema),
    required: z.array(z.string()),
    additionalProperties: z.union([z.boolean(), ToolPropertySchema]).optional(),
  })
  .refine(
    data => {
      // Verify whether the required parameters are defined in the properties.
      for (const requiredParam of data.required) {
        if (!(requiredParam in data.properties)) {
          return false;
        }
      }
      return true;
    },
    {
      message: "Required parameters must be defined in properties",
      path: ["required"],
    },
  );

// ============================================================================
// Tool Metadata Schema
// ============================================================================

/**
 * Tool Metadata Schema
 */
export const ToolMetadataSchema = z.object({
  category: z.string().optional(),
  tags: z.array(z.string()).optional(),
  documentationUrl: z.string().url().optional(),
  customFields: z.record(z.string(), z.any()).optional(),
});

// ============================================================================
// Tool Config Schemas
// ============================================================================

/**
 * Stateless Tool Config Schema
 */
export const StatelessToolConfigSchema = z.object({
  execute: z.function(),
  version: z.string().optional(),
  description: z.string().optional(),
});

/**
 * Stateful Tool Factory Schema
 */
export const StatefulToolFactorySchema = z.object({
  create: z.function(),
});

/**
 * Stateful Tool Config Schema
 */
export const StatefulToolConfigSchema = z.object({
  factory: StatefulToolFactorySchema,
});

/**
 * REST Tool Config Schema
 */
export const RestToolConfigSchema = z.object({
  baseUrl: z.string().url().optional(),
  headers: z.record(z.string(), z.string()).optional(),
  timeout: z.number().positive().optional(),
  maxRetries: z.number().nonnegative().optional(),
  retryDelay: z.number().nonnegative().optional(),
});

/**
 * Built-in Tool Config Schema
 */
export const BuiltinToolConfigSchema = z.object({
  name: z.string().min(1, "Built-in tool name is required"),
  execute: z.function(),
});

/**
 * Tool Config Schema (union type)
 */
export const ToolConfigSchema = z.union([
  StatelessToolConfigSchema,
  StatefulToolConfigSchema,
  RestToolConfigSchema,
  BuiltinToolConfigSchema,
]);

// ============================================================================
// Tool Type Schema
// ============================================================================

/**
 * Tool Type Schema
 */
export const ToolTypeSchema = z.custom<ToolType>((val): val is ToolType =>
  ["STATELESS", "STATEFUL", "REST", "BUILTIN"].includes(val as ToolType),
);

// ============================================================================
// Tool Definition Schema
// ============================================================================

/**
 * Tool Definition Schema
 */
export const ToolDefinitionSchema = z
  .object({
    id: z.string().min(1, "Tool ID is required"),
    name: z.string().min(1, "Tool name is required"),
    type: ToolTypeSchema,
    description: z.string().min(1, "Tool description is required"),
    parameters: ToolParametersSchema,
    metadata: ToolMetadataSchema.optional(),
    config: ToolConfigSchema.optional(),
    createCheckpoint: z.union([z.boolean(), z.enum(["before", "after", "both"])]).optional(),
    checkpointDescriptionTemplate: z.string().optional(),
    strict: z.boolean().optional(),
  })
  .refine(
    data => {
      // Verify the config field based on the type of tool.
      switch (data.type) {
        case "STATELESS":
          return StatelessToolConfigSchema.safeParse(data.config).success;
        case "STATEFUL":
          return StatefulToolConfigSchema.safeParse(data.config).success;
        case "REST":
          // The `config` for REST tools is optional.
          return data.config ? RestToolConfigSchema.safeParse(data.config).success : true;
        case "BUILTIN":
          return BuiltinToolConfigSchema.safeParse(data.config).success;
        default:
          return false;
      }
    },
    {
      message: "Tool configuration is invalid for the specified type",
      path: ["config"],
    },
  );

// ============================================================================
// Tool Schema Schema (for LLM)
// ============================================================================

/**
 * Tool Schema Schema (for LLM)
 */
export const ToolSchemaSchema = z.object({
  id: z.string().min(1, "Tool ID is required"),
  description: z.string().min(1, "Tool description is required"),
  parameters: ToolParametersSchema,
});

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Type guard for ToolProperty
 */
export const isToolProperty = (config: unknown): config is z.infer<typeof ToolPropertySchema> => {
  return ToolPropertySchema.safeParse(config).success;
};

/**
 * Type guard for ToolParameterSchema
 */
export const isToolParameters = (
  config: unknown,
): config is z.infer<typeof ToolParametersSchema> => {
  return ToolParametersSchema.safeParse(config).success;
};

/**
 * Type guard for ToolMetadata
 */
export const isToolMetadata = (config: unknown): config is z.infer<typeof ToolMetadataSchema> => {
  return ToolMetadataSchema.safeParse(config).success;
};

/**
 * Type guard for StatelessToolConfig
 */
export const isStatelessToolConfig = (
  config: unknown,
): config is z.infer<typeof StatelessToolConfigSchema> => {
  return StatelessToolConfigSchema.safeParse(config).success;
};

/**
 * Type guard for StatefulToolConfig
 */
export const isStatefulToolConfig = (
  config: unknown,
): config is z.infer<typeof StatefulToolConfigSchema> => {
  return StatefulToolConfigSchema.safeParse(config).success;
};

/**
 * Type guard for RestToolConfig
 */
export const isRestToolConfig = (
  config: unknown,
): config is z.infer<typeof RestToolConfigSchema> => {
  return RestToolConfigSchema.safeParse(config).success;
};

/**
 * Type guard for BuiltinToolConfig
 */
export const isBuiltinToolConfig = (
  config: unknown,
): config is z.infer<typeof BuiltinToolConfigSchema> => {
  return BuiltinToolConfigSchema.safeParse(config).success;
};

/**
 * Type guard for Tool
 */
export const isTool = (config: unknown): config is z.infer<typeof ToolDefinitionSchema> => {
  return ToolDefinitionSchema.safeParse(config).success;
};
