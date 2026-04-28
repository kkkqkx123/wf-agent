/**
 * Tool Parameter Schema Description Template
 * A schema description used to generate descriptions for tool parameters
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Tool Parameters: Schema Description Template
 */
export const TOOL_PARAMETERS_SCHEMA_TEMPLATE: PromptTemplate = {
  id: "tools.parameters.schema",
  name: "Tool Parameters Schema Description",
  description: "Tool parameters schema description template",
  category: "tools",
  content: `Tool name: {{toolName}}
Tool ID: {{toolId}}
Tool description: {{toolDescription}}

Parameter schema:
\`\`\`json
{{parametersSchema}}
\`\`\`

Parameter description:
{{parametersDescription}}`,
  variables: [
    { name: "toolName", type: "string", required: true, description: "Tool Name" },
    { name: "toolId", type: "string", required: true, description: "Tool ID" },
    { name: "toolDescription", type: "string", required: true, description: "Tool Description" },
    {
      name: "parametersSchema",
      type: "string",
      required: true,
      description: "Parameter schema JSON string",
    },
    {
      name: "parametersDescription",
      type: "string",
      required: false,
      description: "Parameter description text",
    },
  ],
};

/**
 * Parameter description line template (string constant)
 */
export const PARAMETER_DESCRIPTION_LINE_TEMPLATE =
  "- {{paramName}} ({{paramType}}): {{paramDescription}} {{required}}";
