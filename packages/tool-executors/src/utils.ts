/**
 * Tool Executor Auxiliary Functions
 */

import type {
  Tool,
  ToolParameterSchema,
  ToolType,
  ToolOutput,
  RestToolConfig,
} from "@wf-agent/types";

/**
 * Tool Definition (simplified format for use at the app layer)
 * Used for converting tool definitions from the app layer format to the SDK Tool format.
 */
export interface ToolDefinitionLike {
  /** Unique Tool Identifier */
  id: string;
  /** Tool Name */
  name: string;
  /** Tool Description */
  description: string;
  /** Parameter schema (in JSON Schema format) */
  parameters: ToolParameterSchema;
  /** Tool Type */
  type: ToolType;
  /** Version number (optional) */
  version?: string;
  /** Execute the function (stateless tool) */
  execute?: (parameters: Record<string, unknown>) => Promise<ToolOutput>;
  /** Factory function (stateful utility) */
  factory?: () => { execute: (parameters: Record<string, unknown>) => Promise<ToolOutput> };
  /** Configuration (REST tool) */
  config?: RestToolConfig;
}

/**
 * Convert the tool definition to SDK Tool format
 * @param toolDef: The tool definition at the app level
 * @returns: In SDK Tool format
 */
export function toSdkTool(toolDef: ToolDefinitionLike): Tool {
  const tool: Tool = {
    id: toolDef.id,
    name: toolDef.name,
    type: toolDef.type,
    description: toolDef.description,
    parameters: toolDef.parameters,
  };

  // Set the config according to the type.
  if (toolDef.type === "STATELESS" && toolDef.execute) {
    tool.config = {
      execute: toolDef.execute,
      version: toolDef.version,
      description: toolDef.description,
    };
  } else if (toolDef.type === "STATEFUL" && toolDef.factory) {
    tool.config = {
      factory: {
        create: toolDef.factory,
      },
    };
  } else if (toolDef.type === "REST" && toolDef.config) {
    // The REST tool directly uses the provided config.
    tool.config = toolDef.config;
  }

  return tool;
}

/**
 * The batch conversion tool is defined in the SDK Tool format.
 * @param toolDefs: An array of application-layer tool definitions
 * @returns: An array in the SDK Tool format
 */
export function toSdkTools(toolDefs: ToolDefinitionLike[]): Tool[] {
  return toolDefs.map(toSdkTool);
}
