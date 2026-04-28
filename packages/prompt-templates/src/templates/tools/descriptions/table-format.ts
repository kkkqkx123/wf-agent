/**
 * Tool Description Format Template
 * Tool descriptions for generating table formats
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Tool Description Format Template
 */
export const TOOL_DESCRIPTION_TABLE_TEMPLATE: PromptTemplate = {
  id: "tools.description.table",
  name: "Tool Description Table Format",
  description: "Tool Description Format Template",
  category: "tools",
  content: "| {{toolName}} | {{toolId}} | {{toolDescription}} |",
  variables: [
    { name: "toolName", type: "string", required: true, description: "Tool name" },
    { name: "toolId", type: "string", required: true, description: "Tool ID" },
    { name: "toolDescription", type: "string", required: true, description: "Tool description" },
  ],
};
