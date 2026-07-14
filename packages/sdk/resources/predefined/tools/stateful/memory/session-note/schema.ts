/**
 * `session-note` tool parameter Schema definition
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * `record_note` tool parameters Schema
 */
export const recordNoteSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    content: {
      type: "string",
      description: "The information to record as a note. Be concise but specific.",
    },
    category: {
      type: "string",
      description:
        "Optional category/tag for this note (e.g., 'user_preference', 'project_info', 'decision')",
    },
    summary: {
      type: "string",
      description:
        "Optional brief summary of the note content for quick review. The AI agent should provide a concise one-line summary.",
    },
  },
  required: ["content"],
};

/**
 * recall_notes tool parameters Schema
 */
export const recallNotesSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    category: {
      type: "string",
      description: "Optional: filter notes by category",
    },
  },
  required: [],
};

/**
 * list_categories tool parameters Schema
 */
export const listCategoriesSchema: ToolParameterSchema = {
  type: "object",
  properties: {},
  required: [],
};
