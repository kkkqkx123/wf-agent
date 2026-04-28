/**
 * Skill content template
 * Used to format the complete content of a skill for LLM consumption
 */

/**
 * Skill content template
 * Template for formatting complete skill content
 */
export const SKILL_CONTENT_TEMPLATE = `# Skill: {{skillName}}

**Description:** {{skillDescription}}
{{versionSection}}
---

{{content}}`;

/**
 * Skill version section template
 */
export const SKILL_VERSION_SECTION_TEMPLATE = `

**Version:** {{version}}`;

/**
 * Generate skill content prompt
 * @param skill Skill metadata and content
 * @returns Formatted skill content prompt string
 */
export function generateSkillContentPrompt(skill: {
  name: string;
  description: string;
  version?: string;
  content: string;
}): string {
  let prompt = SKILL_CONTENT_TEMPLATE.replace("{{skillName}}", skill.name)
    .replace("{{skillDescription}}", skill.description)
    .replace("{{content}}", skill.content);

  if (skill.version) {
    prompt = prompt.replace(
      "{{versionSection}}",
      SKILL_VERSION_SECTION_TEMPLATE.replace("{{version}}", skill.version),
    );
  } else {
    prompt = prompt.replace("{{versionSection}}", "");
  }

  return prompt;
}
