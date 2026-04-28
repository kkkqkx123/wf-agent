/**
 * Tool JSON Format Template
 * Used to convert tool definitions to JSON text format (for models that do not support Function Call)
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Individual tool JSON format templates
 */
export const TOOL_JSON_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.json",
  name: "Tool JSON Format",
  description: "Tool JSON Format Template",
  category: "tools",
  content: `### {{toolName}}

{{toolDescription}}

Parameters:
{{parametersDescription}}`,
  variables: [
    { name: "toolName", type: "string", required: true, description: "Tool name" },
    { name: "toolDescription", type: "string", required: true, description: "Tool description" },
    {
      name: "parametersDescription",
      type: "string",
      required: false,
      description: "Parameters description",
    },
  ],
};

/**
 * Tool List JSON Format Templates
 */
export const TOOLS_JSON_LIST_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.json_list",
  name: "Tools JSON List",
  description: "Tool List JSON Format Template",
  category: "tools",
  content: `## Available Tools

{{toolsJson}}

### Tool Call Format

Use the following format to call tools:

\`\`\`
<<<TOOL_CALL>>>
{"tool": "tool_name", "parameters": {"parameter_name": "parameter_value"}}
<<<END_TOOL_CALL>>>
\`\`\``,
  variables: [
    { name: "toolsJson", type: "string", required: true, description: "JSON list of tools" },
  ],
};

/**
 * Tool calls JSON parameter line templates
 */
export const TOOL_JSON_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.json_parameter_line",
  name: "Tool JSON Parameter Line",
  description: "Tool JSON Parameter Line Template",
  category: "tools",
  content: `- {{paramName}} ({{paramType}}){{required}}: {{paramDescription}}`,
  variables: [
    { name: "paramName", type: "string", required: true, description: "Parameter name" },
    { name: "paramType", type: "string", required: true, description: "Parameter type" },
    { name: "required", type: "string", required: false, description: "Required flag" },
    {
      name: "paramDescription",
      type: "string",
      required: false,
      description: "Parameter description",
    },
  ],
};
