/**
 * GetSkill Tool Description Template
 * Template for the get_skill tool description
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * GetSkill Tool Description Template
 * Used for generating the description of the get_skill tool
 */
export const GET_SKILL_TOOL_DESCRIPTION_TEMPLATE: PromptTemplate = {
  id: "tools.description.get_skill",
  name: "GetSkill Tool Description",
  description: "GetSkill tool description template",
  category: "tools",
  content: `Load the complete content of a specified skill.

Use this tool when you need detailed information about a skill, including:
- Full skill documentation and instructions
- Reference materials
- Example code
- Available scripts

The skill content will be loaded and returned as formatted text that you can use to guide your work.`,
  variables: [],
};

/**
 * GetSkill Tool Parameter Description Template
 * Template for the skill_name parameter description
 */
export const GET_SKILL_PARAMETER_DESCRIPTION_TEMPLATE: PromptTemplate = {
  id: "tools.parameters.get_skill.skill_name",
  name: "GetSkill Parameter Description",
  description: "GetSkill tool parameter description template",
  category: "tools",
  content: 'The name of the skill to load (e.g., "frontend-design", "pdf-processing")',
  variables: [],
};
