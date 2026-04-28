/**
 * Open tab templates
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Open tab templates
 */
export const OPEN_TABS_TEMPLATE: PromptTemplate = {
  id: "dynamic.open-tabs",
  name: "Open Tabs",
  description: "Opened tab page template",
  category: "dynamic",
  content: `Currently open files in editor:
{{#each tabs}}
  - {{this}}
{{/each}}
{{#if hasMore}}
  ... and {{remainingCount}} more files
{{/if}}`,
  variables: [
    { name: "tabs", type: "array", required: true, description: "Open tabs list" },
    { name: "hasMore", type: "boolean", required: false, description: "Has more tabs" },
    { name: "remainingCount", type: "number", required: false, description: "Remaining count" },
  ],
};
