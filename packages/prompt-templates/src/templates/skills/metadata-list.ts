/**
 * Skill metadata list template
 * Used to generate a list of available skills for system prompts
 */

/**
 * Skill metadata list template header
 */
export const SKILL_METADATA_LIST_HEADER = `## Available Skills

The following skills are available for use. Use the \`get_skill\` tool to load the complete content of a skill when needed.

{{skillList}}

To use a skill, call the \`get_skill\` tool with the skill name.`;

/**
 * Skill metadata item template
 * Template for a single skill metadata entry
 */
export const SKILL_METADATA_ITEM_TEMPLATE = "- `{{skillName}}`: {{skillDescription}}{{version}}";

/**
 * Skill version template
 * Template for skill version suffix
 */
export const SKILL_VERSION_TEMPLATE = " (v{{version}})";

/**
 * Generate skill metadata list prompt
 * @param skills Array of skill metadata
 * @returns Formatted skill list prompt string
 */
export function generateSkillMetadataListPrompt(
  skills: Array<{
    name: string;
    description: string;
    version?: string;
  }>,
): string {
  if (skills.length === 0) {
    return "";
  }

  const skillList = skills
    .map(skill => {
      let item = SKILL_METADATA_ITEM_TEMPLATE.replace("{{skillName}}", skill.name).replace(
        "{{skillDescription}}",
        skill.description,
      );

      if (skill.version) {
        item = item.replace(
          "{{version}}",
          SKILL_VERSION_TEMPLATE.replace("{{version}}", skill.version),
        );
      } else {
        item = item.replace("{{version}}", "");
      }

      return item;
    })
    .join("\n");

  return SKILL_METADATA_LIST_HEADER.replace("{{skillList}}", skillList);
}
