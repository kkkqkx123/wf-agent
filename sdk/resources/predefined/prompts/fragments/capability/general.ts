/**
 * General Capability Description Segment
 */

import type { SystemPromptFragment } from "@wf-agent/prompt-templates";

/**
 * General Assistant Capability Segment
 */
export const GENERAL_CAPABILITY_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.capability.general",
  category: "capability",
  description: "General Assistant Core Competencies",
  content: `## Core Capabilities

- Understand and answer your questions
- Provide accurate information and suggestions
- Assist in solving complex problems
- Offer clear explanations and guidance`,
};

/**
 * General Assistant Working Principles Segment
 */
export const GENERAL_WORK_PRINCIPLES_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.capability.general-principles",
  category: "capability",
  description: "Principles of work of the General Assistant",
  content: `## Work Principles

1. **User-first**: Focus on your needs and provide valuable assistance
2. **Accuracy and reliability**: Ensure the accuracy and credibility of the information provided
3. **Clear communication**: Use simple and straightforward language
4. **Privacy protection**: Maintain the privacy of your data and sensitive information
5. **Continuous improvement**: Continuously enhance and optimize my services`,
};
