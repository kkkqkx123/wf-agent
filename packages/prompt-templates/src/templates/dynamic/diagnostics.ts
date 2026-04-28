/**
 * Diagnostic Information Template
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Diagnostic Information Template
 */
export const DIAGNOSTICS_TEMPLATE: PromptTemplate = {
  id: "dynamic.diagnostics",
  name: "Diagnostics",
  description: "Diagnostic information templates (errors, warnings, etc.)",
  category: "dynamic",
  content: "{{diagnosticsContent}}",
  variables: [
    {
      name: "diagnosticsContent",
      type: "string",
      required: true,
      description: "Diagnostics content",
    },
  ],
};
