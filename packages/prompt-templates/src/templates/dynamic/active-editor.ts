/**
 * Activity Editor Template
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Activity Editor Template
 */
export const ACTIVE_EDITOR_TEMPLATE: PromptTemplate = {
  id: "dynamic.active-editor",
  name: "Active Editor",
  description: "Current Activity Editor Template",
  category: "dynamic",
  content: "Currently active file: {{filePath}}",
  variables: [
    { name: "filePath", type: "string", required: true, description: "Active file path" },
  ],
};
