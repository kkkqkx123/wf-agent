/**
 * JsonNoteStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { JsonNoteStorage, type NoteEntry } from "../json-note-storage.js";

const TEST_DIR = path.join(process.cwd(), "test-notes");

describe("JsonNoteStorage", () => {
  let storage: JsonNoteStorage;

  beforeEach(async () => {
    await fs.mkdir(TEST_DIR, { recursive: true });
    storage = new JsonNoteStorage({
      baseDir: TEST_DIR,
      enableFileLock: true,
    });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    try {
      await fs.rm(TEST_DIR, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  describe("saveNotes and loadNotes", () => {
    it("should save and load notes correctly", async () => {
      const sessionId = "test-session";
      const notes: NoteEntry[] = [
        { timestamp: "2024-01-01T00:00:00Z", category: "general", content: "Note 1" },
        { timestamp: "2024-01-02T00:00:00Z", category: "important", content: "Note 2" },
      ];

      await storage.saveNotes(sessionId, notes);
      const loaded = await storage.loadNotes(sessionId);

      expect(loaded).toEqual(notes);
    });

    it("should return empty array for non-existent session", async () => {
      const loaded = await storage.loadNotes("non-existent");
      expect(loaded).toEqual([]);
    });

    it("should overwrite existing notes", async () => {
      const sessionId = "test-session";
      const notes1: NoteEntry[] = [
        { timestamp: "2024-01-01T00:00:00Z", category: "general", content: "Note 1" },
      ];
      const notes2: NoteEntry[] = [
        { timestamp: "2024-01-02T00:00:00Z", category: "important", content: "Note 2" },
      ];

      await storage.saveNotes(sessionId, notes1);
      await storage.saveNotes(sessionId, notes2);
      const loaded = await storage.loadNotes(sessionId);

      expect(loaded).toEqual(notes2);
    });
  });

  describe("deleteNotes", () => {
    it("should delete notes correctly", async () => {
      const sessionId = "test-session";
      const notes: NoteEntry[] = [
        { timestamp: "2024-01-01T00:00:00Z", category: "general", content: "Note 1" },
      ];

      await storage.saveNotes(sessionId, notes);
      await storage.deleteNotes(sessionId);
      const loaded = await storage.loadNotes(sessionId);

      expect(loaded).toEqual([]);
    });

    it("should not throw when deleting non-existent session", async () => {
      await expect(storage.deleteNotes("non-existent")).resolves.not.toThrow();
    });
  });

  describe("hasNotes", () => {
    it("should return true for existing session", async () => {
      const sessionId = "test-session";
      await storage.saveNotes(sessionId, []);

      expect(await storage.hasNotes(sessionId)).toBe(true);
    });

    it("should return false for non-existent session", async () => {
      expect(await storage.hasNotes("non-existent")).toBe(false);
    });
  });

  describe("getNoteCount", () => {
    it("should return correct count", async () => {
      const sessionId = "test-session";
      const notes: NoteEntry[] = [
        { timestamp: "2024-01-01T00:00:00Z", category: "general", content: "Note 1" },
        { timestamp: "2024-01-02T00:00:00Z", category: "important", content: "Note 2" },
      ];

      await storage.saveNotes(sessionId, notes);

      expect(await storage.getNoteCount(sessionId)).toBe(2);
    });

    it("should return 0 for non-existent session", async () => {
      expect(await storage.getNoteCount("non-existent")).toBe(0);
    });
  });

  describe("multiple sessions", () => {
    it("should handle multiple sessions independently", async () => {
      const session1 = "session-1";
      const session2 = "session-2";

      const notes1: NoteEntry[] = [
        { timestamp: "2024-01-01T00:00:00Z", category: "general", content: "Session 1 Note" },
      ];
      const notes2: NoteEntry[] = [
        { timestamp: "2024-01-02T00:00:00Z", category: "important", content: "Session 2 Note" },
      ];

      await storage.saveNotes(session1, notes1);
      await storage.saveNotes(session2, notes2);

      const loaded1 = await storage.loadNotes(session1);
      const loaded2 = await storage.loadNotes(session2);

      expect(loaded1).toEqual(notes1);
      expect(loaded2).toEqual(notes2);
    });
  });
});
