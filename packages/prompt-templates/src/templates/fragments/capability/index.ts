/**
 * Ability Description Fragment Template
 *
 * Provides a structured template for describing capabilities
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Ability Description Fragment Structure Template
 * Used to describe the core capabilities of an AI assistant
 */
export const CAPABILITY_FRAGMENT_STRUCTURE: PromptTemplate = {
  id: "fragments.capability.structure",
  name: "Capability Fragment Structure",
  description: "Structure template for competency statement fragments",
  category: "fragments",
  content: `## {{sectionTitle}}
{{capabilities}}`,
  variables: [
    {
      name: "sectionTitle",
      type: "string",
      required: true,
      description: 'Chapter titles, such as "Core Capabilities" or "Core Capabilities"',
    },
    {
      name: "capabilities",
      type: "string",
      required: true,
      description: "Capability list (Markdown format)",
    },
  ],
};

/**
 * Ability description snippet constants
 */
export const CAPABILITY_FRAGMENT_TEMPLATE = `## {{sectionTitle}}
{{capabilities}}`;

/**
 * Ability List Item Template
 */
export const CAPABILITY_ITEM_TEMPLATE = `- {{capabilityName}}: {{capabilityDescription}}`;

/**
 * Numbering capability list item template
 */
export const CAPABILITY_NUMBERED_ITEM_TEMPLATE = `{{number}}. **{{capabilityName}}**: {{capabilityDescription}}`;
