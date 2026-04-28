/**
 * Unified template export
 *
 * Note: The system and user-commands templates have been moved to the resources/predefined/prompts directory within the SDK.
 * This package only retains the general template infrastructure (type definitions, rendering functions, and generic format templates).
 */

// Rule template
export * from "./rules/index.js";

// Tool-related templates
export * from "./tools/index.js";

// Skill-related templates
export * from "./skills/index.js";

// System Prompt Word Fragment Template (Structure Definition)
export * from "./fragments/index.js";

// Dynamic Context Template
export * from "./dynamic/index.js";
