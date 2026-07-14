/**
 * The `session-note` tool follows the following execution logic:
 */

import { resolve } from "path";
import type { ToolOutput } from "@wf-agent/types";
import { SqliteNoteStorage } from "@wf-agent/storage";
import type { NoteEntry, NoteCategorySummary } from "./types.js";
import type { SessionNoteConfig } from "../../../types.js";
import { estimateTokens } from "@sdk/utils/token-estimator.js";

/** Module-level shared storage singleton */
let storageInstance: SqliteNoteStorage | null = null;
let storageDbPath: string | null = null;
let storageMaxNotes: number = 1000;

/**
 * Get or create the shared SqliteNoteStorage instance.
 * Reuses the existing instance if the dbPath and maxNotes match.
 */
function getOrCreateStorage(config: SessionNoteConfig): SqliteNoteStorage {
  const dbPath = resolveDbPath(config);
  const maxNotes = config.maxNotes ?? 1000;

  if (storageInstance && storageDbPath === dbPath && storageMaxNotes === maxNotes) {
    return storageInstance;
  }

  // Close existing instance if config changed
  if (storageInstance) {
    storageInstance.close();
  }

  storageInstance = new SqliteNoteStorage({ dbPath, maxNotesPerSession: maxNotes });
  storageInstance.initialize();
  storageDbPath = dbPath;
  storageMaxNotes = maxNotes;
  return storageInstance;
}

/**
 * Close the shared storage singleton.
 * Safe to call multiple times — no-op when already closed.
 */
export function closeStorage(): void {
  if (storageInstance) {
    storageInstance.close();
    storageInstance = null;
    storageDbPath = null;
    storageMaxNotes = 1000;
  }
}

/**
 * Clean up session notes for a specific execution.
 * Deletes all notes associated with the given executionId from the storage.
 * No-op if the storage has not been initialized yet.
 */
export function cleanupSessionNotes(executionId: string): void {
  if (!storageInstance) return;
  storageInstance.clearSession(executionId);
}

/**
 * Resolve the database path from SessionNoteConfig.
 * If dbPath is relative, resolve it against the workspace directory.
 */
function resolveDbPath(config: SessionNoteConfig): string {
  const workspaceDir = config.workspaceDir ?? process.cwd();
  return resolve(workspaceDir, config.dbPath ?? "data/session-notes.db");
}

/**
 * Get the session identifier from config.
 */
function resolveSessionId(config: SessionNoteConfig): string {
  return config.sessionId ?? "default";
}

/**
 * Format notes into a human-readable string
 */
function formatNotes(notes: NoteEntry[]): string {
  return notes
    .map((note, index) => {
      let line = `${index + 1}. [${note.category}]`;
      if (note.summary) {
        line += ` ${note.summary}`;
      } else {
        line += ` ${note.content.length > 100 ? note.content.slice(0, 100) + "..." : note.content}`;
      }
      line += ` (${note.tokenCount} tokens, ${note.timestamp})`;
      return line;
    })
    .join("\n");
}

/**
 * Format categories summary into a human-readable string
 */
function formatCategories(categories: NoteCategorySummary[], totalNotes: number, totalTokens: number): string {
  const lines = [
    `Session Notes Summary: ${totalNotes} notes, ${totalTokens} total tokens`,
    "",
  ];

  for (const cat of categories) {
    lines.push(`  [${cat.category}] ${cat.count} notes, ${cat.totalTokens} tokens`);
  }

  return lines.join("\n");
}

/**
 * Create a record_note tool factory
 */
export function createRecordNoteFactory(config: SessionNoteConfig = {}) {
  const storage = getOrCreateStorage(config);
  const defaultSessionId = resolveSessionId(config);

  return (executionId?: string) => {
    const sessionId = executionId ?? defaultSessionId;
    return {
      execute: async (params: Record<string, unknown>): Promise<ToolOutput> => {
        try {
          const content = params["content"] as string;
          const category = (params["category"] as string) || "general";
          const summary = (params["summary"] as string) || "";
          const tokenCount = estimateTokens(content + (summary ? ` ${summary}` : ""));

          const result = storage.saveNote(sessionId, {
            category,
            content,
            summary,
            tokenCount,
            timestamp: new Date().toISOString(),
          });

          const parts = [
            `Recorded note: ${content} (category: ${category}, ${tokenCount} tokens)`,
            `Note ID: ${result.id}`,
          ];
          if (summary) {
            parts.push(`Summary: ${summary}`);
          }
          return {
            success: true,
            content: parts.join("\n"),
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: `Failed to record note: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      },
      destroy: () => {
        if (executionId) {
          storage.clearSession(sessionId);
        }
      },
    };
  };
}

/**
 * Create a recall_notes tool factory
 */
export function createRecallNotesFactory(config: SessionNoteConfig = {}) {
  const storage = getOrCreateStorage(config);
  const defaultSessionId = resolveSessionId(config);

  return (executionId?: string) => {
    const sessionId = executionId ?? defaultSessionId;
    return {
      execute: async (params: Record<string, unknown>): Promise<ToolOutput> => {
        try {
          const category = params["category"] as string | undefined;

          const notes = storage.listNotes(sessionId, category ? { category } : undefined);

          if (notes.length === 0) {
            const msg = category
              ? `No notes found in category: ${category}`
              : "No notes recorded yet.";
            return { success: true, content: msg };
          }

          const entries: NoteEntry[] = notes.map((n) => ({
            id: n.id,
            timestamp: n.timestamp,
            category: n.category,
            content: n.content,
            summary: n.summary,
            tokenCount: n.tokenCount,
            createdAt: n.createdAt,
            updatedAt: n.updatedAt,
          }));

          const header = category
            ? `Recorded Notes (category: ${category}):\n`
            : "Recorded Notes:\n";
          return {
            success: true,
            content: header + formatNotes(entries),
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: `Failed to recall notes: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      },
      destroy: () => {
        if (executionId) {
          storage.clearSession(sessionId);
        }
      },
    };
  };
}

/**
 * Create a list_categories tool factory
 */
export function createListCategoriesFactory(config: SessionNoteConfig = {}) {
  const storage = getOrCreateStorage(config);
  const defaultSessionId = resolveSessionId(config);

  return (executionId?: string) => {
    const sessionId = executionId ?? defaultSessionId;
    return {
      execute: async (): Promise<ToolOutput> => {
        try {
          const stats = storage.getStats(sessionId);

          if (stats.total_notes === 0) {
            return { success: true, content: "No notes recorded yet." };
          }

          const categories: NoteCategorySummary[] = stats.categories.map((c) => ({
            category: c.category,
            count: c.count,
            totalTokens: c.total_tokens,
          }));

          return {
            success: true,
            content: formatCategories(categories, stats.total_notes, stats.total_tokens),
          };
        } catch (error) {
          return {
            success: false,
            content: "",
            error: `Failed to list categories: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      },
      destroy: () => {
        if (executionId) {
          storage.clearSession(sessionId);
        }
      },
    };
  };
}