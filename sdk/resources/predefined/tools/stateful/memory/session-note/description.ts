/**
 * session-note Tool Description Definition
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const RECORD_NOTE_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "record_note",
  id: "record_note",
  type: "STATEFUL",
  category: "memory",
  description:
    "Record important information as session notes for future reference. Use this to record key facts, user preferences, decisions, or context that should be recalled later in the agent execution chain. Each note is timestamped.",
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
  ],
  tips: [
    "Be concise but specific when recording notes",
    "Use categories to organize related notes",
    "Record key facts, preferences, and decisions",
  ],
};

export const RECALL_NOTES_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "recall_notes",
  id: "recall_notes",
  type: "STATEFUL",
  category: "memory",
  description:
    "Recall all previously recorded session notes. Use this to retrieve important information, context, or decisions from earlier in the session or previous agent execution chains.",
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
  ],
};
