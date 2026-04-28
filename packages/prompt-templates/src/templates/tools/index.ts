/**
 * Unified export of tool-related templates
 */

// Visibility-related templates
export {
  TOOL_VISIBILITY_DECLARATION_TEMPLATE,
  TOOL_TABLE_ROW_TEMPLATE,
  VISIBILITY_CHANGE_TYPE_TEXTS,
} from "./visibility/declaration.js";
export type { VisibilityChangeType } from "./visibility/declaration.js";

// Tool Description Render Functions
export {
  renderToolDescription,
  renderToolDescriptionSingleLine,
  renderToolDescriptionListItem,
  renderToolDescriptionTableRow,
  renderToolDescriptions,
} from "./tool-description.js";

// Description of relevant templates
export {
  TOOL_DESCRIPTION_TABLE_TEMPLATE,
  GET_SKILL_TOOL_DESCRIPTION_TEMPLATE,
  GET_SKILL_PARAMETER_DESCRIPTION_TEMPLATE,
} from "./descriptions/index.js";

// Parameter-related templates
export {
  TOOL_PARAMETERS_SCHEMA_TEMPLATE,
  PARAMETER_DESCRIPTION_LINE_TEMPLATE,
} from "./parameters/schema.js";

// Format Conversion Templates
export {
  TOOL_XML_FORMAT_TEMPLATE,
  TOOLS_XML_LIST_TEMPLATE,
  TOOL_XML_PARAMETER_LINE_TEMPLATE,
  TOOL_JSON_FORMAT_TEMPLATE,
  TOOLS_JSON_LIST_TEMPLATE,
  TOOL_JSON_PARAMETER_LINE_TEMPLATE,
  TOOL_RAW_FORMAT_TEMPLATE,
  TOOLS_RAW_LIST_TEMPLATE,
  TOOL_RAW_PARAMETER_LINE_TEMPLATE,
  TOOL_RAW_COMPACT_TEMPLATE,
  TOOLS_RAW_COMPACT_LIST_TEMPLATE,
} from "./formatters/index.js";
