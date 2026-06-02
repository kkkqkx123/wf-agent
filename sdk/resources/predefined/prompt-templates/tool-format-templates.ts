import type { PromptTemplate } from "@wf-agent/types";

export const TOOL_XML_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tool-xml-format",
  name: "Tool XML Format",
  description: "Single tool description in XML format",
  category: "tools",
  content: `<tool name="{{toolName}}">
<description>{{toolDescription}}</description>
<parameters>
{{parametersDescription}}
</parameters>
</tool>`,
};

export const TOOLS_XML_LIST_TEMPLATE: PromptTemplate = {
  id: "tools-xml-list",
  name: "Tools XML List",
  description: "Tool list with XML call format instructions",
  category: "tools",
  content: `## Available Tools

The following tools are available for use:

{{toolsXml}}

### Tool Call Format

Use the following format to call tools:

\`\`\`xml
<function_calls>
<invoke name="tool_name">
<parameter name="parameter_name">parameter_value</parameter>
</invoke>
</function_calls>
\`\`\``,
};

export const TOOL_XML_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tool-xml-parameter-line",
  name: "Tool XML Parameter Line",
  description: "Single parameter description line in XML format",
  category: "tools",
  content: "- {{paramName}} ({{paramType}}){{required}}: {{paramDescription}}",
};

export const TOOL_JSON_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tool-json-format",
  name: "Tool JSON Format",
  description: "Single tool description in JSON format",
  category: "tools",
  content: `Tool Name: {{toolName}}
Description: {{toolDescription}}
Parameters (JSON Schema):
\`\`\`json
{{parametersSchema}}
\`\`\``,
};

export const TOOLS_JSON_LIST_TEMPLATE: PromptTemplate = {
  id: "tools-json-list",
  name: "Tools JSON List",
  description: "Tool list with JSON call format instructions",
  category: "tools",
  content: `## Available Tools

The following tools are available for use:

{{toolsJson}}

### Tool Call Format

When calling tools, respond with a JSON object:

\`\`\`json
{
  "function": "tool_name",
  "parameters": {
    "parameter_name": "parameter_value"
  }
}
\`\`\``,
};

export const TOOL_JSON_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tool-json-parameter-line",
  name: "Tool JSON Parameter Line",
  description: "Single parameter description line in JSON format",
  category: "tools",
  content: "- {{paramName}}: {{paramDescription}} ({{paramType}}){{required}}",
};

export const TOOL_RAW_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tool-raw-format",
  name: "Tool Raw Format",
  description: "Single tool description in raw format",
  category: "tools",
  content: `Tool: {{toolName}}
Description: {{toolDescription}}
Parameters: {{parametersDescription}}`,
};

export const TOOLS_RAW_LIST_TEMPLATE: PromptTemplate = {
  id: "tools-raw-list",
  name: "Tools Raw List",
  description: "Tool list in raw format",
  category: "tools",
  content: `Available Tools:
{{toolsRaw}}`,
};

export const TOOL_RAW_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tool-raw-parameter-line",
  name: "Tool Raw Parameter Line",
  description: "Single parameter description line in raw format",
  category: "tools",
  content: "- {{paramName}}: {{paramDescription}} ({{paramType}}){{required}}",
};

export const TOOL_RAW_COMPACT_TEMPLATE: PromptTemplate = {
  id: "tool-raw-compact",
  name: "Tool Raw Compact",
  description: "Single tool description in compact raw format",
  category: "tools",
  content: "{{toolName}}: {{toolDescription}}",
};

export const TOOLS_RAW_COMPACT_LIST_TEMPLATE: PromptTemplate = {
  id: "tools-raw-compact-list",
  name: "Tools Raw Compact List",
  description: "Tool list in compact raw format",
  category: "tools",
  content: "Available tools: {{toolsRawCompact}}",
};