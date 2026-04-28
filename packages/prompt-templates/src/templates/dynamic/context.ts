/**
 * Dynamic Context Templates
 *
 * Defines the overall structure of a dynamic context message
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Dynamic Context Prefix Templates
 */
export const DYNAMIC_CONTEXT_PREFIX_TEMPLATE: PromptTemplate = {
  id: "dynamic.context.prefix",
  name: "Dynamic Context Prefix",
  description: "Explanation of dynamic context prefixes",
  category: "dynamic",
  content: `This is the current turn's dynamic context information you can use. It may change between turns. Continue with the previous task if the information is not needed and ignore it.`,
  variables: [],
};

/**
 * Paragraph-wrapping templates
 */
export const SECTION_WRAPPER_TEMPLATE: PromptTemplate = {
  id: "dynamic.context.section-wrapper",
  name: "Section Wrapper",
  description: "Paragraph wrapping template",
  category: "dynamic",
  content: `====

{{title}}

{{content}}`,
  variables: [
    { name: "title", type: "string", required: true, description: "Section title" },
    { name: "content", type: "string", required: true, description: "Section content" },
  ],
};
