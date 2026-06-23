/**
 * The `list_files` tool is used for export.
 */

export { listFilesSchema } from "./schema.js";
export { createListFilesHandler } from "./handler.js";
export { createListFilesDescription } from "./description.js";
// Re-export with default config for backward compatibility (tool-descriptions.ts)
import { createListFilesDescription } from "./description.js";
export const LIST_FILES_TOOL_DESCRIPTION = createListFilesDescription();
