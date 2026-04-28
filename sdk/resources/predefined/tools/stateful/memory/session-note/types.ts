/**
 * session-note Tool Status Type Definition
 */

/**
 * Note entry
 */
export interface NoteEntry {
  timestamp: string;
  category: string;
  content: string;
}

/**
 * Session note instance status
 */
export interface SessionNoteState {
  memoryFile: string;
  notes: NoteEntry[];
  loaded: boolean;
}
