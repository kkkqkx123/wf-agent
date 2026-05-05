/**
 * Tests for AgentLoopDeltaRestorer
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopDeltaRestorer } from "../agent-loop-delta-restorer.js";
import type { AgentLoopCheckpoint, AgentLoopStateSnapshot, Message } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopDeltaRestorer", () => {
  let loadCheckpoint: ReturnType<typeof vi.fn>;
  let listCheckpoints: ReturnType<typeof vi.fn>;
  let restorer: AgentLoopDeltaRestorer;

  beforeEach(() => {
    loadCheckpoint = vi.fn();
    listCheckpoints = vi.fn();
    restorer = new AgentLoopDeltaRestorer(
      loadCheckpoint as unknown as (id: string) => Promise<AgentLoopCheckpoint | null>,
      listCheckpoints as unknown as (_agentLoopId: string) => Promise<string[]>,
    );
  });

  const createFullCheckpoint = (id: string): AgentLoopCheckpoint => ({
    id,
    agentLoopId: "agent-1",
    timestamp: Date.now(),
    type: "FULL",
    snapshot: {
      status: AgentLoopStatus.RUNNING,
      currentIteration: 1,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
      error: undefined,
      messages: [{ role: "user", content: "Hello" } as Message],
      variables: { key: "value" },
      config: {},
    },
  });

  const createDeltaCheckpoint = (
    id: string,
    baseId: string,
    previousId: string,
  ): AgentLoopCheckpoint => ({
    id,
    agentLoopId: "agent-1",
    timestamp: Date.now(),
    type: "DELTA",
    baseCheckpointId: baseId,
    previousCheckpointId: previousId,
    delta: {
      addedMessages: [{ role: "assistant", content: "Hi" } as Message],
      statusChange: { from: AgentLoopStatus.RUNNING, to: AgentLoopStatus.COMPLETED },
    },
  });

  describe("restore", () => {
    it("should restore from a full checkpoint", async () => {
      const checkpoint = createFullCheckpoint("cp-1");
      loadCheckpoint.mockResolvedValue(checkpoint);

      const result = await restorer.restore("cp-1");

      expect(result.snapshot).toEqual(checkpoint.snapshot);
      expect(result.metadata.messages).toEqual(checkpoint.snapshot?.messages);
      expect(result.metadata.variables).toEqual(checkpoint.snapshot?.variables);
    });

    it("should throw error when checkpoint has no snapshot", async () => {
      const checkpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;
      loadCheckpoint.mockResolvedValue(checkpoint);

      await expect(restorer.restore("cp-1")).rejects.toThrow();
    });

    it("should restore from delta checkpoint chain", async () => {
      const baseCheckpoint = createFullCheckpoint("base-1");
      const deltaCheckpoint = createDeltaCheckpoint("delta-1", "base-1", "base-1");

      loadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "delta-1") return deltaCheckpoint;
        if (id === "base-1") return baseCheckpoint;
        return null;
      });

      const result = await restorer.restore("delta-1");

      expect(result.snapshot.status).toBe(AgentLoopStatus.COMPLETED);
      expect(result.snapshot.messages.length).toBe(2);
    });

    it("should apply delta correctly", async () => {
      const baseCheckpoint = createFullCheckpoint("base-1");
      const deltaCheckpoint: AgentLoopCheckpoint = {
        id: "delta-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "DELTA",
        baseCheckpointId: "base-1",
        previousCheckpointId: "base-1",
        delta: {
          addedMessages: [
            { role: "assistant", content: "Response" } as Message,
            { role: "user", content: "Follow-up" } as Message,
          ],
          modifiedVariables: new Map([["newKey", "newValue"]]),
        },
      };

      loadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "delta-1") return deltaCheckpoint;
        if (id === "base-1") return baseCheckpoint;
        return null;
      });

      const result = await restorer.restore("delta-1");

      expect(result.snapshot.messages.length).toBe(3);
      expect(result.snapshot.variables.newKey).toBe("newValue");
    });

    it("should handle complex delta with otherChanges", async () => {
      const baseCheckpoint = createFullCheckpoint("base-1");
      const deltaCheckpoint: AgentLoopCheckpoint = {
        id: "delta-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "DELTA",
        baseCheckpointId: "base-1",
        previousCheckpointId: "base-1",
        delta: {
          otherChanges: {
            toolCallCount: { from: 0, to: 5 },
            endTime: { from: undefined, to: Date.now() },
          },
        },
      };

      loadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "delta-1") return deltaCheckpoint;
        if (id === "base-1") return baseCheckpoint;
        return null;
      });

      const result = await restorer.restore("delta-1");

      expect(result.snapshot.toolCallCount).toBe(5);
    });

    it("should throw error when base checkpoint not found", async () => {
      const deltaCheckpoint = createDeltaCheckpoint("delta-1", "base-1", "base-1");
      loadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "delta-1") return deltaCheckpoint;
        return null;
      });

      await expect(restorer.restore("delta-1")).rejects.toThrow();
    });
  });

  describe("extractSnapshot", () => {
    it("should extract snapshot from full checkpoint", () => {
      const checkpoint = createFullCheckpoint("cp-1");
      const snapshot = (restorer as any).extractSnapshot(checkpoint);

      expect(snapshot).toEqual(checkpoint.snapshot);
    });

    it("should throw error when snapshot is missing", () => {
      const checkpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;

      expect(() => (restorer as any).extractSnapshot(checkpoint)).toThrow();
    });
  });

  describe("hasSnapshot", () => {
    it("should return true for full checkpoint with snapshot", () => {
      const checkpoint = createFullCheckpoint("cp-1");
      expect((restorer as any).hasSnapshot(checkpoint)).toBe(true);
    });

    it("should return false for checkpoint without snapshot", () => {
      const checkpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;
      expect((restorer as any).hasSnapshot(checkpoint)).toBe(false);
    });
  });

  describe("extractParentId", () => {
    it("should extract agentLoopId as parent ID", () => {
      const checkpoint = createFullCheckpoint("cp-1");
      const parentId = (restorer as any).extractParentId(checkpoint);

      expect(parentId).toBe("agent-1");
    });
  });

  describe("createRestoreResult", () => {
    it("should create restore result with metadata", () => {
      const snapshot: AgentLoopStateSnapshot = {
        status: AgentLoopStatus.COMPLETED,
        currentIteration: 5,
        toolCallCount: 10,
        startTime: Date.now(),
        endTime: Date.now(),
        error: undefined,
        messages: [{ role: "user", content: "Test" } as Message],
        variables: { test: "value" },
        config: {},
      };

      const result = (restorer as any).createRestoreResult(snapshot);

      expect(result.snapshot).toEqual(snapshot);
      expect(result.metadata.messages).toEqual(snapshot.messages);
      expect(result.metadata.variables).toEqual(snapshot.variables);
      expect(result.metadata.config).toEqual(snapshot.config);
    });
  });
});
