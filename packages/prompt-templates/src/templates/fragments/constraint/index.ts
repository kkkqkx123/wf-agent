/**
 * Constraint Condition Fragment Template
 *
 * Provide a structural template for constraints
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Constraint Fragment Structure Template
 * Used to describe the constraints and rules for AI assistants
 */
export const CONSTRAINT_FRAGMENT_STRUCTURE: PromptTemplate = {
  id: "fragments.constraint.structure",
  name: "Constraint Fragment Structure",
  description: "Structural templates for constraint fragments",
  category: "fragments",
  content: `## {{sectionTitle}}
{{constraints}}`,
  variables: [
    {
      name: "sectionTitle",
      type: "string",
      required: true,
      description: 'Section title, e.g., "Constraints" or "Constraints".',
    },
    {
      name: "constraints",
      type: "string",
      required: true,
      description: "List of constraints (Markdown format)",
    },
  ],
};

/**
 * Constraint condition fragment constants
 */
export const CONSTRAINT_FRAGMENT_TEMPLATE = `## {{sectionTitle}}
{{constraints}}`;

/**
 * Security Rule Fragment Structure Template
 */
export const SAFETY_CONSTRAINT_STRUCTURE: PromptTemplate = {
  id: "fragments.constraint.safety.structure",
  name: "Safety Constraint Structure",
  description: "Structural templates for security rule fragments",
  category: "fragments",
  content: `## {{sectionTitle}}

### Data Security
{{dataSecurity}}

### Code Security
{{codeSecurity}}

### Error Handling
{{errorHandling}}`,
  variables: [
    { name: "sectionTitle", type: "string", required: true, description: "Chapter Title" },
    { name: "dataSecurity", type: "string", required: true, description: "Data security rules" },
    { name: "codeSecurity", type: "string", required: true, description: "Code Security Rules" },
    { name: "errorHandling", type: "string", required: true, description: "Error Handling Rules" },
  ],
};
