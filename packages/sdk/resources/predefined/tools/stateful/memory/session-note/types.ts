/**
 * session-note Tool Status Type Definition
 */

/**
 * Note entry
 */
export interface NoteEntry {
  id: string;
  timestamp: string;
  category: string;
  content: string;
  summary: string;
  tokenCount: number;
  createdAt: number;
  updatedAt: number;
}

/**
 * Category summary with aggregated statistics
 */
export interface NoteCategorySummary {
  category: string;
  count: number;
  totalTokens: number;
}

/**
 * Session note instance status
 */
export interface SessionNoteState {
  notes: NoteEntry[];
  loaded: boolean;
}
