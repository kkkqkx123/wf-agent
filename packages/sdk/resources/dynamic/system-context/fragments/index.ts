/**
 * System Context Fragments (stable)
 *
 * Fine-grained generators for stable system context:
 * - Current time
 * - Environment information
 * - Available tools
 * - Skills and workflows
 */

export { generateCurrentTimeSection, generateCurrentTimeContent } from "./current-time.js";
export { generateEnvironmentSection, getDefaultEnvironmentInfo } from "./environment.js";
export {
  generateAvailableToolsContent,
  generateToolDescriptionMessage,
  generateCompactToolsContent,
  generateToolDocumentation,
} from "./available-tools.js";
export { generateSkillsContent } from "./skills.js";
export { generateWorkflowsContent } from "./workflows.js";
export { wrapSection, cleanupEmptyLines } from "./utils.js";

