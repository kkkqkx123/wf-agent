/**
 * Unified export of clip templates
 *
 * Provide structural templates for system prompt snippets (placeholders and basic formatting only)
 * Actual business content defined at SDK level
 */

// Role Definition Fragment Structure
export * from "./role/index.js";

// Competency statement fragment structure
export * from "./capability/index.js";

// Constraint fragment structure
export * from "./constraint/index.js";

// Tool usage specification fragment structure
export * from "./tool-usage/index.js";

// Clip Assembly Tool
export * from "./composer.js";
