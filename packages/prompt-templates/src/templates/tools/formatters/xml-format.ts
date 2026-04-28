/**
 * Tool XML format template
 * Used to convert tool definitions into XML format (suitable for models that do not support Function Call)
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Single Tool XML Format Template
 */
export const TOOL_XML_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.xml",
  name: "Tool XML Format",
  description: "Tools XML format templates",
  category: "tools",
  content: `<tool name="{{toolName}}">
<description>{{toolDescription}}</description>
<parameters>
{{parametersDescription}}
</parameters>
</tool>`,
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
 * Tool List XML Format Template
 */
export const TOOLS_XML_LIST_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.xml_list",
  name: "Tools XML List",
  description: "Tool list XML format template",
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
  variables: [
    { name: "toolsXml", type: "string", required: true, description: "XML list of tools" },
  ],
};

/**
 * Tool Call XML Parameter Line Template
 */
export const TOOL_XML_PARAMETER_LINE_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.xml_parameter_line",
  name: "Tool XML Parameter Line",
  description: "Tool XML Parameter Line Template",
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
