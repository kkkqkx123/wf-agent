/**
 * Template ID constant
 * Defines a unique identifier for all templates to avoid hard-coding strings.
 */

/**
 * Collection of template ID constants
 */
export const TEMPLATE_IDS = {
  /** System prompt word template ID */
  SYSTEM: {
    /** Programmer system prompt words */
    CODER: "system.coder",
    /** Assistant system prompt words */
    ASSISTANT: "system.assistant",
  },
  /** Rule template ID */
  RULES: {
    /** Format rules */
    FORMAT: "rules.format",
    /** Security Rules */
    SAFETY: "rules.safety",
  },
  /** User Instruction Template ID */
  USER_COMMANDS: {
    /** Code Review Instructions */
    CODE_REVIEW: "user_commands.code_review",
    /** Data analysis instructions */
    DATA_ANALYSIS: "user_commands.data_analysis",
  },
  /** Tool-related template ID */
  TOOLS: {
    /** Tool Visibility Statement */
    VISIBILITY_DECLARATION: "tools.visibility.declaration",
    /** Tool Description Table Format */
    DESCRIPTION_TABLE: "tools.description.table",
    /** Tool Parameter Schema Description */
    PARAMETERS_SCHEMA: "tools.parameters.schema",
    /** GetSkill Tool Description */
    GET_SKILL_DESCRIPTION: "tools.description.get_skill",
    /** GetSkill parameter description */
    GET_SKILL_PARAMETER: "tools.parameters.get_skill.skill_name",
  },
  /** Fragment ID */
  FRAGMENTS: {
    /** Tool Visibility Fragment */
    TOOL_VISIBILITY: "fragment.tool_visibility",
  },
} as const;

/**
 * Template ID Type
 */
export type TemplateId = (typeof TEMPLATE_IDS)[keyof typeof TEMPLATE_IDS];
