import { describe, it, expect } from "vitest";
import {
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
  isConnected,
  isDisabled,
  getServerDisplayName,
} from "../shared/connection-state.js";
import type { McpServerState } from "../shared/types.js";

function makeState(overrides: Partial<McpServerState> = {}): McpServerState {
  return {
    name: "test-server",
    config: JSON.stringify({ type: "stdio", command: "echo" }),
    status: "disconnected",
    disabled: false,
    source: "global",
    errorHistory: [],
    ...overrides,
  };
}

describe("Connection State Management", () => {
  describe("createInitialServerState", () => {
    it("should create with disconnected status", () => {
      const state = createInitialServerState(
        "my-server",
        '{"type":"stdio","command":"test"}',
        "project",
        "/project/path",
      );
      expect(state.name).toBe("my-server");
      expect(state.status).toBe("disconnected");
      expect(state.disabled).toBe(false);
      expect(state.source).toBe("project");
      expect(state.projectPath).toBe("/project/path");
      expect(state.errorHistory).toEqual([]);
    });

    it("should create without projectPath", () => {
      const state = createInitialServerState("s", "{}", "global");
      expect(state.projectPath).toBeUndefined();
    });
  });

  describe("updateServerStatus", () => {
    it("should update status and return new object", () => {
      const state = makeState();
      const updated = updateServerStatus(state, "connected");
      expect(updated.status).toBe("connected");
      expect(state.status).toBe("disconnected"); // Original unchanged
    });
  });

  describe("addErrorToHistory", () => {
    it("should add error to history", () => {
      const state = makeState();
      const updated = addErrorToHistory(state, "Connection failed");
      expect(updated.errorHistory).toHaveLength(1);
      expect(updated.errorHistory[0]!.message).toBe("Connection failed");
      expect(updated.errorHistory[0]!.level).toBe("error");
      expect(typeof updated.errorHistory[0]!.timestamp).toBe("number");
    });

    it("should set current error", () => {
      const state = makeState();
      const updated = addErrorToHistory(state, "Failed");
      expect(updated.error).toBe("Failed");
    });

    it("should truncate long error messages", () => {
      const state = makeState();
      const longMsg = "x".repeat(2000);
      const updated = addErrorToHistory(state, longMsg);
      expect(updated.error!.length).toBeLessThanOrEqual(1000 + 14); // 1000 + "(truncated)"
      expect(updated.error!).toContain("(truncated)");
    });

    it("should respect maxHistory", () => {
      const state = makeState({ errorHistory: [] });
      let updated = state;
      for (let i = 0; i < 10; i++) {
        updated = addErrorToHistory(updated, `Error ${i}`, "error", 5);
      }
      expect(updated.errorHistory).toHaveLength(5); // Only last 5 kept
    });

    it("should support warn and info levels", () => {
      const state = makeState();
      const warn = addErrorToHistory(state, "Warning", "warn");
      expect(warn.errorHistory[0]!.level).toBe("warn");

      const info = addErrorToHistory(state, "Info", "info");
      expect(info.errorHistory[0]!.level).toBe("info");
    });
  });

  describe("clearErrorState", () => {
    it("should clear error field", () => {
      const state = makeState({ error: "Some error" });
      const updated = clearErrorState(state);
      expect(updated.error).toBeUndefined();
      // errorHistory is preserved
      expect(updated.errorHistory).toBeDefined();
    });
  });

  describe("isConnectable", () => {
    it("should return true when not disabled and not connecting", () => {
      expect(isConnectable(makeState())).toBe(true);
    });

    it("should return false when disabled", () => {
      expect(isConnectable(makeState({ disabled: true }))).toBe(false);
    });

    it("should return false when connecting", () => {
      expect(isConnectable(makeState({ status: "connecting" }))).toBe(false);
    });
  });

  describe("isConnected", () => {
    it("should return true when status is connected", () => {
      expect(isConnected(makeState({ status: "connected" }))).toBe(true);
    });

    it("should return false when not connected", () => {
      expect(isConnected(makeState())).toBe(false);
      expect(isConnected(makeState({ status: "connecting" }))).toBe(false);
    });
  });

  describe("isDisabled", () => {
    it("should return true when disabled is true", () => {
      expect(isDisabled(makeState({ disabled: true }))).toBe(true);
    });

    it("should return false when disabled is false", () => {
      expect(isDisabled(makeState())).toBe(false);
    });
  });

  describe("getServerDisplayName", () => {
    it("should return server name for global source", () => {
      expect(getServerDisplayName(makeState({ source: "global" }))).toBe("test-server");
    });

    it("should append (project) for project source", () => {
      expect(getServerDisplayName(makeState({ source: "project" }))).toBe("test-server (project)");
    });
  });
});
