/**
 * The `glob` tool is used for export.
 */

export { globSchema } from "./schema.js";
export { createGlobHandler } from "./handler.js";
export { createGlobDescription } from "./description.js";
// Re-export with default config for backward compatibility (tool-descriptions.ts)
import { createGlobDescription } from "./description.js";
export const GLOB_TOOL_DESCRIPTION = createGlobDescription();
