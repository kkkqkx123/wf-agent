/**
 * Unified export of tool functions for general use
 */

// Tool description generator (with enhanced features)
export {
  generateToolDescription,
  generateToolListDescription,
  generateToolAvailabilitySection,
  generateToolTableRow,
  generateToolTable,
  getToolDescriptionData,
  convertToolToDescriptionData,
  type ToolDescriptionFormat,
  type ToolWithDescription,
} from "./tool-description-generator.js";

// Tool description registry for looking up predefined tool descriptions
export {
  ToolDescriptionRegistry,
  toolDescriptionRegistry,
  registerToolDescription,
  registerToolDescriptions,
  getToolDescription,
  hasToolDescription,
} from "./tool-description-registry.js";

// Tool parameter converter (JSON Schema -> ToolParameterDescription)
export {
  convertToolParameters,
  convertToolParametersToString,
  extractParameterNames,
  isParameterRequired,
  getParameterDefault,
} from "./tool-parameter-converter.js";

// Other tool utilities
export * from "./tool-parameters-describer.js";
export * from "./tool-schema-cleaner.js";
export * from "./tool-schema-formatter.js";
