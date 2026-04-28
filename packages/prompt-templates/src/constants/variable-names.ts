/**
 * Variable names and constants
 * Define the variable names used in the templates to avoid hard-coding strings.
 */

/**
 * Variable Name Constant Collection
 */
export const VARIABLE_NAMES = {
  /** Tool-related variables */
  TOOL: {
    /** Tool name */
    NAME: "toolName",
    /** Tool ID */
    ID: "toolId",
    /** Tool Description */
    DESCRIPTION: "toolDescription",
  },
  /** Visibility-related variables */
  VISIBILITY: {
    /** commencement date */
    TIMESTAMP: "timestamp",
    /** Scope Types */
    SCOPE: "scope",
    /** Scope ID */
    SCOPE_ID: "scopeId",
    /** Change type text */
    CHANGE_TYPE_TEXT: "changeTypeText",
    /** Tool Description Form Rows */
    TOOL_DESCRIPTIONS: "toolDescriptions",
  },
  /** Parameters-related variables */
  PARAMETERS: {
    /** Parameter name */
    NAME: "paramName",
    /** Parameter Types */
    TYPE: "paramType",
    /** Parameter Description */
    DESCRIPTION: "paramDescription",
    /** Parameter Schema */
    SCHEMA: "parametersSchema",
    /** Parameter description */
    DESCRIPTION_TEXT: "parametersDescription",
  },
  /** User input variable */
  USER: {
    /** user input */
    INPUT: "user_input",
  },
} as const;

/**
 * Variable Name Type
 */
export type VariableName = (typeof VARIABLE_NAMES)[keyof typeof VARIABLE_NAMES];
