/**
 * session-note Tool Description Definition
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const RECORD_NOTE_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "record_note",
  type: "STATEFUL",
  category: "memory",
  description:
    "Record important information as session notes for future reference. Use this to record key facts, user preferences, decisions, or context that should be recalled later in the agent execution chain. Each note is timestamped and can include a summary for quick review.",
  parameters: [
    {
      name: "content",
      type: "string",
      required: true,
      description: "The information to record as a note. Be concise but specific.",
    },
    {
      name: "category",
      type: "string",
      required: false,
      description:
        "Optional category/tag for this note (e.g., 'user_preference', 'project_info', 'decision')",
    },
    {
      name: "summary",
      type: "string",
      required: false,
      description:
        "Optional brief summary of the note content for quick review. Provide a concise one-line summary.",
    },
  ],
  tips: [
    "Be concise but specific when recording notes",
    "Use categories to organize related notes",
    "Provide a summary for quick reference in category listings",
    "Record key facts, preferences, and decisions",
  ],
};

export const RECALL_NOTES_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "recall_notes",
  type: "STATEFUL",
  category: "memory",
  description:
    "Recall all previously recorded session notes. Use this to retrieve important information, context, or decisions from earlier in the session or previous agent execution chains. Each note includes its summary and estimated token count.",
  parameters: [
    {
      name: "category",
      type: "string",
      required: false,
      description: "Optional: filter notes by category",
    },
  ],
  tips: [
    "Use category filter to find specific types of notes",
    "Call this at the start of a new session to get context",
    "Review token counts to manage context window usage",
  ],
};

export const LIST_CATEGORIES_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "list_categories",
  type: "STATEFUL",
  category: "memory",
  description:
    "List all note categories with their entry count and total estimated token count. Use this to get an overview of how notes are organized and the total token usage per category.",
  parameters: [],
  tips: [
    "Use this to see which categories have notes",
    "Check total tokens per category to manage memory usage",
    "Combine with recall_notes with category filter for targeted retrieval",
  ],
};
