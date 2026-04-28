/**
 * Tool type definition unified export
 * Define the basic information and parameter schema of the tool
 */

// Export Tool Definitions
export * from "./definition.js";

// Export Status Type
export * from "./state.js";

// Export Tool Static Configuration Type (JSON Schema format)
export {
  type ToolProperty,
  type ToolParameterSchema,
  type ToolMetadata,
  type ApprovalConditionType,
  type ApprovalCondition,
} from "./static-config.js";

// Export Tool Runtime Configuration Type (execution-time configuration)
export {
  type ToolConfig,
  type StatelessToolConfig,
  type StatefulToolConfig,
  type RestToolConfig,
  type BuiltinToolConfig,
  type StatefulToolInstance,
  type StatefulToolFactory,
  type BuiltinToolExecutionContext,
  ToolExecutionResult as BuiltinToolExecutionResult,
  ToolParameterSchema as ToolRuntimeParameters,
} from "./runtime-config.js";

// Export execution-related types
export * from "./execution.js";

// Export Tool Approval Related Types
export * from "./approval.js";

// Export Risk Level Types
export * from "./risk-level.js";

// Export File Permission Types
export * from "./file-permission.js";

// Export MCP Approval Types
export * from "./mcp-approval.js";

// Export Zod Schemas for Tool Validation
export {
  ToolPropertySchema,
  ToolParametersSchema,
  ToolMetadataSchema,
  StatelessToolConfigSchema,
  StatefulToolFactorySchema,
  StatefulToolConfigSchema,
  RestToolConfigSchema,
  BuiltinToolConfigSchema,
  ToolConfigSchema,
  ToolTypeSchema,
  ToolDefinitionSchema,
  ToolSchemaSchema,
  isToolProperty,
  isToolParameters,
  isToolMetadata,
  isStatelessToolConfig,
  isStatefulToolConfig,
  isRestToolConfig,
  isBuiltinToolConfig,
  isTool,
} from "./tool-schema.js";