/**
 * JSON Note Storage Implementation for Session Notes
 * Lightweight storage for session notes using JSON file backend
 */

import { BaseJsonStorage, type BaseJsonStorageConfig } from "./base-json-storage.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("note-storage");

/**
 * Note entry structure
 */
export interface NoteEntry {
  timestamp: string;
  category: string;
  content: string;
}

/**
 * Note metadata
 */
export interface NoteMetadata {
  /** Number of notes */
  count: number;
  /** Last updated timestamp */
  updatedAt: number;
}

/**
 * JSON Note Storage
 * Specialized storage for session notes with simple array-based data model
 */
export class JsonNoteStorage extends BaseJsonStorage<NoteMetadata> {
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Save notes for a session
   * @param sessionId Session identifier
   * @param notes Array of note entries
   */
  async saveNotes(sessionId: string, notes: NoteEntry[]): Promise<void> {
    const data = new TextEncoder().encode(JSON.stringify(notes));
    const metadata: NoteMetadata = {
      count: notes.length,
      updatedAt: Date.now(),
    };

    await this.save(sessionId, data, metadata);

    logger.debug("Notes saved", { sessionId, count: notes.length });
  }

  /**
   * Load notes for a session
   * @param sessionId Session identifier
   * @returns Array of note entries, empty array if not found
   */
  async loadNotes(sessionId: string): Promise<NoteEntry[]> {
    const data = await this.load(sessionId);

    if (!data) {
      logger.debug("No notes found for session", { sessionId });
      return [];
    }

    try {
      const notes = JSON.parse(new TextDecoder().decode(data)) as NoteEntry[];
      logger.debug("Notes loaded", { sessionId, count: notes.length });
      return notes;
    } catch (error) {
      logger.error("Failed to parse notes", { sessionId, error: (error as Error).message });
      return [];
    }
  }

  /**
   * Delete notes for a session
   * @param sessionId Session identifier
   */
  async deleteNotes(sessionId: string): Promise<void> {
    await this.delete(sessionId);
    logger.debug("Notes deleted", { sessionId });
  }

  /**
   * Check if notes exist for a session
   * @param sessionId Session identifier
   */
  async hasNotes(sessionId: string): Promise<boolean> {
    return await this.exists(sessionId);
  }

  /**
   * Get note count for a session
   * @param sessionId Session identifier
   * @returns Note count, 0 if not found
   */
  async getNoteCount(sessionId: string): Promise<number> {
    const metadata = await this.getMetadata(sessionId);
    return metadata?.count ?? 0;
  }
}
