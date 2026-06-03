import type { PromptTemplate } from "@wf-agent/types";

export const TOOL_PARAMETERS_SCHEMA_TEMPLATE: PromptTemplate = {
  id: "tool-parameters-schema",
  name: "Tool Parameters Schema",
  description: "Tool parameters with schema and description list",
  category: "tools",
  content: `Tool: {{toolName}} ({{toolId}})
Description: {{toolDescription}}

Parameter schema:
\`\`\`json
{{parametersSchema}}
\`\`\`

Parameter description:
{{parametersDescription}}`,
};

export const PARAMETER_DESCRIPTION_LINE_TEMPLATE: PromptTemplate = {
  id: "parameter-description-line",
  name: "Parameter Description Line",
  description: "Single parameter description line",
  category: "tools",
  content: "- {{paramName}} ({{paramType}}): {{paramDescription}} {{required}}",
};