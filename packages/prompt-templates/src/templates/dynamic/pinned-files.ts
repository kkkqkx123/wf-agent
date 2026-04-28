/**
 * Fix the file template
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Fix the file template
 */
export const PINNED_FILES_TEMPLATE: PromptTemplate = {
  id: "dynamic.pinned-files",
  name: "Pinned Files",
  description: "Fixed file content template",
  category: "dynamic",
  content: `Pinned files:
{{#each files}}
  - {{this.path}}
{{/each}}`,
  variables: [{ name: "files", type: "array", required: true, description: "Pinned files list" }],
};
