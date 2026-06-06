/**
 * Unit Tests for Interruption History Manager
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  InterruptionHistoryManager,
  type InterruptionHistoryEntry,
} from "../interruption-history-manager.js";

describe("InterruptionHistoryManager", () => {
  let manager: InterruptionHistoryManager;

  beforeEach(() => {
    manager = new InterruptionHistoryManager();
  });

  describe("record", () => {
    it("should record an entry with generated id and timestamp", () => {
      const entry = manager.record({
        type: "PAUSE",
        contextId: "exec-1",
        triggeredBy: "user",
      });

      expect(entry.id).toBeDefined();
      expect(entry.id).toContain("int_hist_");
      expect(entry.timestamp).toBeGreaterThan(0);
      expect(entry.type).toBe("PAUSE");
      expect(entry.contextId).toBe("exec-1");
      expect(entry.triggeredBy).toBe("user");
    });

    it("should record multiple entries", () => {
      manager.record({ type: "PAUSE", contextId: "exec-1" });
      manager.record({ type: "RESUME", contextId: "exec-1" });
      manager.record({ type: "STOP", contextId: "exec-2" });

      expect(manager.getSize()).toBe(3);
    });

    it("should enforce max size limit", () => {
      const smallManager = new InterruptionHistoryManager(2);

      smallManager.record({ type: "PAUSE", contextId: "e1" });
      smallManager.record({ type: "RESUME", contextId: "e1" });
      smallManager.record({ type: "STOP", contextId: "e1" });

      expect(smallManager.getSize()).toBe(2);
    });
  });

  describe("getHistory", () => {
    it("should return all entries sorted newest first", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "RESUME", contextId: "e1" });

      const history = manager.getHistory();
      expect(history.length).toBe(2);
      // Newest first
      expect(history[0].type).toBe("RESUME");
      expect(history[1].type).toBe("PAUSE");
    });

    it("should filter by contextId", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "PAUSE", contextId: "e2" });

      const filtered = manager.getHistory({ contextId: "e1" });
      expect(filtered).toHaveLength(1);
      expect(filtered[0].contextId).toBe("e1");
    });

    it("should filter by type", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "STOP", contextId: "e1" });

      const filtered = manager.getHistory({ type: "PAUSE" });
      expect(filtered).toHaveLength(1);
      expect(filtered[0].type).toBe("PAUSE");
    });

    it("should filter by triggeredBy", () => {
      manager.record({ type: "PAUSE", contextId: "e1", triggeredBy: "user" });
      manager.record({ type: "PAUSE", contextId: "e1", triggeredBy: "system" });

      const filtered = manager.getHistory({ triggeredBy: "user" });
      expect(filtered).toHaveLength(1);
    });

    it("should filter by time range", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });

      const now = Date.now();
      const filtered = manager.getHistory({ since: now - 1000, before: now + 1000 });
      expect(filtered).toHaveLength(1);
    });

    it("should apply limit", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "RESUME", contextId: "e1" });

      const limited = manager.getHistory({ limit: 1 });
      expect(limited).toHaveLength(1);
    });

    it("should return empty array when no match", () => {
      const result = manager.getHistory({ contextId: "non-existent" });
      expect(result).toEqual([]);
    });
  });

  describe("getLatestEvent", () => {
    it("should return the most recent event for a context", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "RESUME", contextId: "e1" });

      const latest = manager.getLatestEvent("e1");
      expect(latest).toBeDefined();
      expect(latest!.type).toBe("RESUME");
    });

    it("should return undefined when no events for context", () => {
      expect(manager.getLatestEvent("non-existent")).toBeUndefined();
    });
  });

  describe("getPauseDuration", () => {
    it("should calculate pause duration between PAUSE and RESUME", async () => {
      manager.record({ type: "PAUSE", contextId: "e1" });

      // Small delay
      await new Promise(resolve => setTimeout(resolve, 10));

      manager.record({ type: "RESUME", contextId: "e1" });

      const duration = manager.getPauseDuration("e1");
      expect(duration).toBeGreaterThanOrEqual(10);
    });

    it("should return null when no PAUSE/RESUME pair", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      expect(manager.getPauseDuration("e1")).toBeNull();
    });

    it("should return null for unknown context", () => {
      expect(manager.getPauseDuration("non-existent")).toBeNull();
    });
  });

  describe("getStatistics", () => {
    it("should return correct statistics for all entries", () => {
      manager.record({ type: "PAUSE", contextId: "e1", triggeredBy: "user" });
      manager.record({ type: "RESUME", contextId: "e1" });
      manager.record({ type: "STOP", contextId: "e2", triggeredBy: "system" });

      const stats = manager.getStatistics();
      expect(stats.totalEvents).toBe(3);
      expect(stats.pauseCount).toBe(1);
      expect(stats.resumeCount).toBe(1);
      expect(stats.stopCount).toBe(1);
      expect(stats.userTriggeredCount).toBe(1);
      expect(stats.systemTriggeredCount).toBe(1);
    });

    it("should filter statistics by contextId", () => {
      manager.record({ type: "PAUSE", contextId: "e1", triggeredBy: "user" });
      manager.record({ type: "PAUSE", contextId: "e2", triggeredBy: "system" });

      const stats = manager.getStatistics("e1");
      expect(stats.totalEvents).toBe(1);
      expect(stats.userTriggeredCount).toBe(1);
    });
  });

  describe("export", () => {
    it("should return a copy of all entries", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      const exported = manager.export();
      expect(exported).toHaveLength(1);
      // Should be a copy, not reference
      expect(exported).not.toBe((manager as any).history);
    });
  });

  describe("clear", () => {
    it("should clear all history", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.clear();
      expect(manager.getSize()).toBe(0);
    });
  });

  describe("setMaxSize", () => {
    it("should trim history when reducing max size", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.record({ type: "RESUME", contextId: "e1" });
      manager.setMaxSize(1);

      expect(manager.getSize()).toBe(1);
    });

    it("should not trim when increasing max size", () => {
      manager.record({ type: "PAUSE", contextId: "e1" });
      manager.setMaxSize(2000);
      expect(manager.getSize()).toBe(1);
    });
  });
});