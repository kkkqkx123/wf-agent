/**
 * ToolSchemaFormatter - A tool for converting tool schemas to different formats
 *
 * Features:
 * - Converts to Function Call format (standard JSON Schema)
 * - Converts to XML format (suitable for models that do not support Function Call)
 * - Converts to JSON text format (suitable for models that do not support Function Call)
 * - Supports batch conversion
 *
 */

import type { Tool, ToolParameterSchema } from "@wf-agent/types";
import { renderTemplate } from "@wf-agent/common-utils";
import {
  TOOL_XML_FORMAT_TEMPLATE,
  TOOLS_XML_LIST_TEMPLATE,
  TOOL_XML_PARAMETER_LINE_TEMPLATE,
  TOOL_JSON_FORMAT_TEMPLATE,
  TOOLS_JSON_LIST_TEMPLATE,
  TOOL_JSON_PARAMETER_LINE_TEMPLATE,
} from "@wf-agent/prompt-templates";

/**
 * LLM Tool Schema Format (Function Call Mode)
 */
export interface LLMToolSchema {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: ToolParameterSchema;
  };
}

/**
 * Tool format type
 */
export type ToolFormatType = "function_call" | "xml" | "json";

/**
 * 将单个工具转换为 Function Call Schema 格式
 *
 * @param tool 工具对象
 * @returns LLM 工具 Schema
 *
 * @example
 * ```ts
 * const tool = {
 *   id: 'calculator',
 *   name: 'calculator',
 *   description: 'Performs calculations',
 *   parameters: { properties: {...}, required: [...] }
 * };
 * const schema = toFunctionCallSchema(tool);
 * // 结果:
 * // {
 * //   type: 'function',
 * //   function: {
 * //     name: 'calculator',
 * //     description: 'Performs calculations',
 * //     parameters: {...}
 * //   }
 * // }
 * ```
 */
export function toFunctionCallSchema(tool: Tool): LLMToolSchema {
  return {
    type: "function",
    function: {
      name: tool.name,
      description: tool.description,
      parameters: tool.parameters,
    },
  };
}

/**
 * Convert the tool list to Function Call Schema format
 *
 * @param tools Array of tools
 * @returns Array of LLM tool Schemas
 */
export function toFunctionCallSchemas(tools: Tool[]): LLMToolSchema[] {
  return tools.map(toFunctionCallSchema);
}

/**
 * Generate parameter descriptions in XML format
 *
 * @param tool The tool object
 * @returns Parameter descriptions in XML format
 */
function generateXMLParametersDescription(tool: Tool): string {
  if (!tool.parameters || Object.keys(tool.parameters.properties).length === 0) {
    return "No parameters";
  }

  const lines: string[] = [];
  const properties = tool.parameters.properties;
  const required = tool.parameters.required || [];

  for (const [paramName, paramDef] of Object.entries(properties)) {
    const isRequired = required.includes(paramName);
    const variables = {
      paramName,
      paramType: paramDef.type,
      paramDescription: paramDef.description || "No description",
      required: isRequired ? " [required]" : "",
    };
    lines.push(renderTemplate(TOOL_XML_PARAMETER_LINE_TEMPLATE.content, variables));
  }

  return lines.join("\n");
}

/**
 * 将单个工具转换为 XML 格式
 *
 * @param tool 工具对象
 * @returns XML 格式的工具描述
 *
 * @example
 * ```ts
 * const tool = {
 *   id: 'calculator',
 *   name: 'calculator',
 *   description: 'Performs calculations',
 *   parameters: {...}
 * };
 * const xml = toXMLFormat(tool);
 * // 结果:
 * // <tool name="calculator">
 * // <description>Performs calculations</description>
 * // <parameters>
 * // - a (number) [required]: First number
 * // - b (number) [required]: Second number
 * // </parameters>
 * // </tool>
 * ```
 */
export function toXMLFormat(tool: Tool): string {
  const variables = {
    toolName: tool.name,
    toolDescription: tool.description,
    parametersDescription: generateXMLParametersDescription(tool),
  };

  return renderTemplate(TOOL_XML_FORMAT_TEMPLATE.content, variables);
}

/**
 * Convert the list of tools into XML format (including instructions on the call format)
 *
 * @param tools Array of tools
 * @returns Description of the tool list in XML format
 */
export function toXMLFormatBatch(tools: Tool[]): string {
  if (!tools || tools.length === 0) {
    return "No tools available";
  }

  const toolsXml = tools.map(toXMLFormat).join("\n\n");

  return renderTemplate(TOOLS_XML_LIST_TEMPLATE.content, { toolsXml });
}

/**
 * Generate parameter descriptions in JSON format
 *
 * @param tool: The tool object
 * @returns: Parameter descriptions in JSON format
 */
function generateJSONParametersDescription(tool: Tool): string {
  if (!tool.parameters || Object.keys(tool.parameters.properties).length === 0) {
    return "No parameters";
  }

  const lines: string[] = [];
  const properties = tool.parameters.properties;
  const required = tool.parameters.required || [];

  for (const [paramName, paramDef] of Object.entries(properties)) {
    const isRequired = required.includes(paramName);
    const variables = {
      paramName,
      paramType: paramDef.type,
      paramDescription: paramDef.description || "No description",
      required: isRequired ? " [required]" : "",
    };
    lines.push(renderTemplate(TOOL_JSON_PARAMETER_LINE_TEMPLATE.content, variables));
  }

  return lines.join("\n");
}

/**
 * Converts a single tool object into JSON text format.
 *
 * @param tool The tool object to be converted.
 * @returns The tool description in JSON text format.
 *
 * @example
 * ```ts
 * const tool = {
 *   id: 'calculator',
 *   name: 'calculator',
 *   description: 'Performs calculations',
 *   parameters: {...}
 * };
 * const json = toJSONFormat/tool);
 * // Result:
 * // ### calculator
 * //
 * // Performs calculations
 * //
 * // Parameters:
 * // - a (number) [required]: First number
 * // - b (number) [required]: Second number
 * ```
 */
export function toJSONFormat(tool: Tool): string {
  const variables = {
    toolName: tool.name,
    toolDescription: tool.description,
    parametersDescription: generateJSONParametersDescription(tool),
  };

  return renderTemplate(TOOL_JSON_FORMAT_TEMPLATE.content, variables);
}

/**
 * Convert the tool list into JSON text format (including instructions on the call format)
 *
 * @param tools Array of tools
 * @returns JSON text format description of the tool list
 */
export function toJSONFormatBatch(tools: Tool[]): string {
  if (!tools || tools.length === 0) {
    return "No tools available";
  }

  const toolsJson = tools.map(toJSONFormat).join("\n\n");

  return renderTemplate(TOOLS_JSON_LIST_TEMPLATE.content, { toolsJson });
}

/**
 * Format Type Conversion Tool
 *
 * @param tool: The tool object
 * @param format: The format type
 * @returns: The converted format
 */
export function formatTool(tool: Tool, format: ToolFormatType): string | LLMToolSchema {
  switch (format) {
    case "function_call":
      return toFunctionCallSchema(tool);
    case "xml":
      return toXMLFormat(tool);
    case "json":
      return toJSONFormat(tool);
    default:
      return toFunctionCallSchema(tool);
  }
}

/**
 * Batch conversion tool based on format type
 *
 * @param tools Array of tools
 * @param format Format type
 * @returns Converted format
 */
export function formatTools(tools: Tool[], format: ToolFormatType): LLMToolSchema[] | string {
  switch (format) {
    case "function_call":
      return toFunctionCallSchemas(tools);
    case "xml":
      return toXMLFormatBatch(tools);
    case "json":
      return toJSONFormatBatch(tools);
    default:
      return toFunctionCallSchemas(tools);
  }
}
