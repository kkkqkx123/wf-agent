/**
 * Programming-related constraints segment
 */

import type { SystemPromptFragment } from "@wf-agent/prompt-templates";

/**
 * Important Notes for Programming Assistants Segment
 */
export const CODING_CONSTRAINTS_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.constraint.coding",
  category: "constraint",
  description: "Programming Assistant Important Notes",
  content: `## Important Notes

- Avoid overdesign; keep code concise
- Consider code scalability and maintainability
- Pay attention to security and privacy protection
- Provide timely feedback on progress and issues`,
};

/**
 * Code Security Rules Segment
 */
export const CODE_SAFETY_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.constraint.code-safety",
  category: "constraint",
  description: "Code Security Rules",
  content: `## Code Safety

- Do not execute code from untrusted sources without review
- Avoid hardcoding sensitive information (passwords, API keys)
- Validate all user inputs before processing
- Follow secure coding practices`,
};
