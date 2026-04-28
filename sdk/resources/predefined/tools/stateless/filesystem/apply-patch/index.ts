/**
 * The `apply_patch` tool module
 *
 * A stripped-down, file-oriented diff format designed to be easy to parse and safe to apply
 * Based on the Codex apply_patch specification
 */

// Public API
export { applyPatchSchema } from "./schema.js";
export { createApplyPatchHandler } from "./handler.js";
export { APPLY_PATCH_TOOL_DESCRIPTION } from "./description.js";
