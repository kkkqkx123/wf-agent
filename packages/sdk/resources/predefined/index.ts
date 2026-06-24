/**
 * Export of predefined content
 *
 * This module provides:
 * 1. Predefined trigger templates and workflow definitions
 * 2. Predefined tool definitions
 * 3. Predefined prompt templates (system prompts, user instructions, etc.)
 * 4. Functions for registering/unregistering tools
 */

// Export the trigger module
export * from "./trigger/index.js";

// Export the workflow module
export * from "./workflow/index.js";

// Export predefined tools
export * from "./tools/index.js";

// Export preset type definitions
export * from "./presets-types.js";

// Export the predefined prompt moduleword templates
export * from "./prompts/index.js";

// Export the registration tool function
export { registerPredefinedContent, unregisterPredefinedContent } from "./registration.js";
