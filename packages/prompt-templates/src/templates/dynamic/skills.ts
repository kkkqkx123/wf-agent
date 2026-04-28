/**
 * Skills templates
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Skills templates
 */
export const SKILLS_TEMPLATE: PromptTemplate = {
  id: "dynamic.skills",
  name: "Active Skills",
  description: "Enabled Skills templates",
  category: "dynamic",
  content: `Active skills:
{{#each skills}}
  - {{this.name}}: {{this.description}}
{{/each}}`,
  variables: [{ name: "skills", type: "array", required: true, description: "Skills list" }],
};
