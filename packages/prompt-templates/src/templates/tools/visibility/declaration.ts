/**
 * Tool visibility declaration template
 * Used to generate notification messages for changes to tool visibility
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Lightweight tool update notification template
 * Only notifies about added/removed tools without exposing internal details
 */
export const TOOL_VISIBILITY_DECLARATION_TEMPLATE: PromptTemplate = {
  id: "tools.visibility.declaration",
  name: "Tool Visibility Update Notification",
  description: "Lightweight notification for tool availability changes",
  category: "tools",
  content: `Available tools updated:
{{addedTools}}
{{removedTools}}`,
  variables: [
    {
      name: "addedTools",
      type: "string",
      required: false,
      description: "List of newly added tools (format: '- tool_id: brief description')",
    },
    {
      name: "removedTools",
      type: "string",
      required: false,
      description: "List of removed tools (format: '- tool_id')",
    },
  ],
};

/**
 * Tool update notification format
 */
export interface ToolUpdateNotification {
  /** Newly added tools with descriptions */
  added?: Array<{ id: string; description: string }>;
  /** Removed tool IDs */
  removed?: string[];
}
