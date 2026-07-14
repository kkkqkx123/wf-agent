/**
 * File Note Storage Implementation for Session Notes
 * Lightweight storage for session notes using simple file I/O.
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { NoteEntry } from "../types.js";

/**
 * File Note Storage
 * Stores session notes as individual JSON files per session.
 * Simple, lightweight, no external dependencies beyond fs.
 */
export class FileNoteStorage {
  private baseDir: string;
  private initialized = false;

  constructor(baseDir: string = "./storage/notes") {
    this.baseDir = baseDir;
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;
    await fs.mkdir(this.baseDir, { recursive: true });
    this.initialized = true;
  }

  private getFilePath(sessionId: string): string {
    // Sanitize sessionId to prevent path traversal
    const safeId = sessionId.replace(/[/\\:*?"<>|]/g, "_");
    return path.join(this.baseDir, `${safeId}.json`);
  }

  /**
   * Save notes for a session
   */
  async saveNotes(sessionId: string, notes: NoteEntry[]): Promise<void> {
    if (!this.initialized) await this.initialize();
    const filePath = this.getFilePath(sessionId);
    await fs.writeFile(filePath, JSON.stringify(notes, null, 2), "utf-8");
  }

  /**
   * Load notes for a session
   */
  async loadNotes(sessionId: string): Promise<NoteEntry[]> {
    if (!this.initialized) await this.initialize();
    const filePath = this.getFilePath(sessionId);
    try {
      const content = await fs.readFile(filePath, "utf-8");
      return JSON.parse(content) as NoteEntry[];
    } catch {
      return [];
    }
  }

  /**
   * Delete notes for a session
   */
  async deleteNotes(sessionId: string): Promise<void> {
    if (!this.initialized) await this.initialize();
    const filePath = this.getFilePath(sessionId);
    try {
      await fs.unlink(filePath);
    } catch {
      // File may not exist
    }
  }

  /**
   * Check if notes exist for a session
   */
  async hasNotes(sessionId: string): Promise<boolean> {
    if (!this.initialized) await this.initialize();
    const filePath = this.getFilePath(sessionId);
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Get note count for a session
   */
  async getNoteCount(sessionId: string): Promise<number> {
    const notes = await this.loadNotes(sessionId);
    return notes.length;
  }
}
