/**
 * The `session-note` tool follows the following execution logic:
 */

import { resolve } from "path";
import type { ToolOutput } from "@wf-agent/types";
import { JsonNoteStorage, type NoteEntry } from "@wf-agent/storage";
import type { SessionNoteConfig } from "../../../types.js";

/**
 * Session note instance using JsonNoteStorage
 */
class SessionNoteInstance {
  private storage: JsonNoteStorage;
  private sessionId: string;
  private notes: NoteEntry[] = [];
  private loaded: boolean = false;

  constructor(storage: JsonNoteStorage, sessionId: string) {
    this.storage = storage;
    this.sessionId = sessionId;
  }

  /**
   * Load notes from storage
   */
  private async loadNotes(): Promise<void> {
    if (this.loaded) return;

    this.notes = await this.storage.loadNotes(this.sessionId);
    this.loaded = true;
  }

  /**
   * Save notes to storage
   */
  private async saveNotes(): Promise<void> {
    await this.storage.saveNotes(this.sessionId, this.notes);
  }

  /**
   * Take notes.
   */
  async record(content: string, category: string = "general"): Promise<ToolOutput> {
    try {
      await this.loadNotes();

      const note: NoteEntry = {
        timestamp: new Date().toISOString(),
        category,
        content,
      };
      this.notes.push(note);

      await this.saveNotes();

      return {
        success: true,
        content: `Recorded note: ${content} (category: ${category})`,
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: `Failed to record note: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  /**
   * Recall Notes
   */
  async recall(category?: string): Promise<ToolOutput> {
    try {
      await this.loadNotes();

      if (this.notes.length === 0) {
        return {
          success: true,
          content: "No notes recorded yet.",
        };
      }

      let filteredNotes = this.notes;
      if (category) {
        filteredNotes = this.notes.filter(n => n.category === category);
        if (filteredNotes.length === 0) {
          return {
            success: true,
            content: `No notes found in category: ${category}`,
          };
        }
      }

      const formatted = filteredNotes.map((note, index) => {
        return `${index + 1}. [${note.category}] ${note.content}\n   (recorded at ${note.timestamp})`;
      });

      return {
        success: true,
        content: "Recorded Notes:\n" + formatted.join("\n"),
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: `Failed to recall notes: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }
}

/**
 * Create a record_note tool factory
 */
export function createRecordNoteFactory(config: SessionNoteConfig = {}) {
  const workspaceDir = resolve(config.workspaceDir ?? process.cwd());
  const sessionId = (config.memoryFile ?? "session-notes").replace(/\.json$/, "");

  return () => {
    const storage = new JsonNoteStorage({
      baseDir: workspaceDir,
      enableFileLock: true,
    });

    return {
      execute: async (params: Record<string, unknown>) => {
        await storage.initialize();
        const instance = new SessionNoteInstance(storage, sessionId);
        return instance.record(params["content"] as string, params["category"] as string);
      },
    };
  };
}

/**
 * Create a recall_notes tool factory
 */
export function createRecallNotesFactory(config: SessionNoteConfig = {}) {
  const workspaceDir = resolve(config.workspaceDir ?? process.cwd());
  const sessionId = (config.memoryFile ?? "session-notes").replace(/\.json$/, "");

  return () => {
    const storage = new JsonNoteStorage({
      baseDir: workspaceDir,
      enableFileLock: true,
    });

    return {
      execute: async (params: Record<string, unknown>) => {
        await storage.initialize();
        const instance = new SessionNoteInstance(storage, sessionId);
        return instance.recall(params["category"] as string | undefined);
      },
    };
  };
}
