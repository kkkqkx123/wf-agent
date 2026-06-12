export const SKILL_METADATA_LIST_HEADER = `## Available Skills

The following skills are available for use. Use the \`get_skill\` tool to load the complete content of a skill when needed.

{{skillList}}

To use a skill, call the \`get_skill\` tool with the skill name.`;

export const SKILL_METADATA_ITEM_TEMPLATE = "- `{{skillName}}`: {{skillDescription}}{{version}}";

export const SKILL_VERSION_TEMPLATE = " (v{{version}})";

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

export const SKILL_CONTENT_TEMPLATE = `# Skill: {{skillName}}

**Description:** {{skillDescription}}
{{versionSection}}
---

{{content}}`;

export const SKILL_VERSION_SECTION_TEMPLATE = `

**Version:** {{version}}`;

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
