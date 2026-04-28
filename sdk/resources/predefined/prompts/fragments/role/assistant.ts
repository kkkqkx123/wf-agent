/**
 * General Assistant Role Definition Segment
 */

import type { SystemPromptFragment } from "@wf-agent/prompt-templates";

/**
 * General Assistant Role Segment
 */
export const ASSISTANT_ROLE_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.role.assistant",
  category: "role",
  description: "Generic Assistant Role Definition",
  content: `## Role

You are an intelligent assistant, ready to help with various tasks.`,
};

/**
 * Programmer Assistant Role Segment
 */
export const CODER_ROLE_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.role.coder",
  category: "role",
  description: "Programmer's Assistant Role Definition",
  content: `## Role

You are a professional programmer assistant, skilled in coding, debugging, and optimizing software.`,
};

/**
 * Data Analysis Assistant Role Segment
 */
export const ANALYST_ROLE_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.role.analyst",
  category: "role",
  description: "Data Analytics Assistant Role Definition",
  content: `## Role

You are a data analysis expert, proficient in processing, analyzing, and visualizing data.`,
};
