/**
 * Tool Raw JSON Format Template
 * Used to convert tool definitions to raw JSON format (without markers)
 * Suitable for models that output raw JSON without special delimiters
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Single Tool Raw JSON Format Template
 */
export const TOOL_RAW_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw",
  name: "Tool Raw JSON Format",
  description: "Raw JSON format template for tool definitions (without markers)",
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
 * Tool List Raw JSON Format Template
 */
export const TOOLS_RAW_LIST_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw_list",
  name: "Tools Raw JSON List",
  description: "Raw JSON tool list with call format instructions",
  category: "tools",
  content: `## Available Tools

{{toolsRaw}}

### Tool Call Format

When you need to use a tool, output a valid JSON object in the following format:

For single tool call:
\`\`\`json
{"tool": "tool_name", "parameters": {"parameter_name": "parameter_value"}}
\`\`\`

For multiple tool calls (output a JSON array):
\`\`\`json
[
  {"tool": "tool1_name", "parameters": {...}},
  {"tool": "tool2_name", "parameters": {...}}
]
\`\`\`

### Important Rules

1. **Output ONLY valid JSON** - Do not include any other text, markdown, or explanations
2. **Use exact tool names** - Match the tool names listed above exactly
3. **Follow parameter schema** - Use the exact parameter names and types specified
4. **No markdown formatting** - Do not wrap the JSON in code blocks when actually calling tools
5. **Single or multiple** - Output a single object for one tool, an array for multiple tools`,
  variables: [
    { name: "toolsRaw", type: "string", required: true, description: "Raw JSON list of tools" },
  ],
};

/**
 * Tool Call Raw JSON Parameter Line Template
 */
export const TOOL_RAW_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw_parameter_line",
  name: "Tool Raw JSON Parameter Line",
  description: "Raw JSON parameter line template",
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

/**
 * Compact Raw JSON Format Template
 * For models with limited context window
 */
export const TOOL_RAW_COMPACT_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw_compact",
  name: "Tool Raw JSON Compact",
  description: "Compact raw JSON format for limited context",
  category: "tools",
  content: `{{toolName}}: {{toolDescription}}
Params: {{parametersSummary}}`,
  variables: [
    { name: "toolName", type: "string", required: true, description: "Tool name" },
    { name: "toolDescription", type: "string", required: true, description: "Tool description" },
    {
      name: "parametersSummary",
      type: "string",
      required: false,
      description: "Compact parameters summary",
    },
  ],
};

/**
 * Compact Raw JSON List Template
 */
export const TOOLS_RAW_COMPACT_LIST_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw_compact_list",
  name: "Tools Raw JSON Compact List",
  description: "Compact raw JSON tool list",
  category: "tools",
  content: `## Tools

{{toolsCompact}}

Call format: {"tool": "name", "parameters": {...}} or [{"tool": "name", ...}] for multiple`,
  variables: [
    { name: "toolsCompact", type: "string", required: true, description: "Compact tool list" },
  ],
};
