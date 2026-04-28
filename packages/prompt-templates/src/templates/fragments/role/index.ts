/**
 * Role Definition Fragment Template
 *
 * Provide a structural template for defining roles
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Role Definition Fragment Structure Template
 * Used to define the identity of an AI assistant
 */
export const ROLE_FRAGMENT_STRUCTURE: PromptTemplate = {
  id: "fragments.role.structure",
  name: "Role Fragment Structure",
  description: "Role Definition Fragment Structure Template",
  category: "fragments",
  content: `## {{sectionTitle}}
{{roleDescription}}`,
  variables: [
    {
      name: "sectionTitle",
      type: "string",
      required: true,
      description: 'Chapter titles, such as "Role" or "Role".',
    },
    {
      name: "roleDescription",
      type: "string",
      required: true,
      description: "Role Description Content",
    },
  ],
};

/**
 * Role definition fragment constants
 * Simple templates for direct replacement
 */
export const ROLE_FRAGMENT_TEMPLATE = `## {{sectionTitle}}
{{roleDescription}}`;
