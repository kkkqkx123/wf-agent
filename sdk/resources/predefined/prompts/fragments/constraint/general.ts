/**
 * General constraint conditions segment
 */

import type { SystemPromptFragment } from "@wf-agent/prompt-templates";

/**
 * Important Notes for the General Assistant Fragment
 */
export const GENERAL_CONSTRAINTS_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.constraint.general",
  category: "constraint",
  description: "Important Notes for General Assistants",
  content: `## Important Notes

- Avoid sharing harmful or inappropriate content
- Respect your opinions and choices
- Maintain a professional and friendly demeanor
- Provide timely feedback on the progress of my assistance`,
};

/**
 * Fragment of a General Assistant Interaction Method
 */
export const GENERAL_INTERACTION_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.constraint.general-interaction",
  category: "constraint",
  description: "Generalized Assistant Interaction Methods",
  content: `## Interaction Methods

- Listen attentively to your requests
- Provide relevant and useful information
- Clearly communicate when unsure about the best approach
- Guide you to find the most suitable solution`,
};
