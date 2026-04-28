/**
 * Workspace file tree template
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Workspace file tree template
 */
export const WORKSPACE_FILES_TEMPLATE: PromptTemplate = {
  id: "dynamic.workspace-files",
  name: "Workspace Files",
  description: "Workspace File Tree Template",
  category: "dynamic",
  content: `The following is a list of files in the current workspace:

{{fileTree}}`,
  variables: [
    { name: "fileTree", type: "string", required: true, description: "File tree content" },
  ],
};
