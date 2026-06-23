/**
 * Conversation Session Execution State Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ConversationSession } from "../conversation-session.js";

describe("ConversationSession - Execution State", () => {
  let session: ConversationSession;

  beforeEach(() => {
    session = new ConversationSession();
  });

  describe("getTurnState", () => {
    it("should return undefined for uncached turn", () => {
      const retrieved = session.getTurnState(0, "dynamicContext");
      expect(retrieved).toBeUndefined();
    });

    it("should return stored state", () => {
      session.setTurnState(0, "dynamicContext", "stored context for turn 0");

      const retrieved = session.getTurnState(0, "dynamicContext");
      expect(retrieved).toBe("stored context for turn 0");
    });

    it("should handle multiple state keys per turn", () => {
      session.setTurnState(0, "contextA", "value A");
      session.setTurnState(0, "contextB", "value B");

      expect(session.getTurnState(0, "contextA")).toBe("value A");
      expect(session.getTurnState(0, "contextB")).toBe("value B");
    });

    it("should handle complex objects as state values", () => {
      const complexObj = { summary: "test", tokens: 100, metadata: { source: "llm" } };
      session.setTurnState(1, "metadata", complexObj);

      const retrieved = session.getTurnState(1, "metadata");
      expect(retrieved).toEqual(complexObj);
    });
  });

  describe("setTurnState", () => {
    it("should store and retrieve state", () => {
      const value = "This is execution state for turn 1";
      session.setTurnState(1, "dynamicContext", value);

      const retrieved = session.getTurnState(1, "dynamicContext");
      expect(retrieved).toBe(value);
    });

    it("should overwrite existing state", () => {
      session.setTurnState(2, "status", "original");
      session.setTurnState(2, "status", "updated");

      const retrieved = session.getTurnState(2, "status");
      expect(retrieved).toBe("updated");
    });
  });

  describe("clearStateFromIndex", () => {
    it("should clear state from specified index onwards", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.setTurnState(1, "ctx", "state 1");
      session.setTurnState(2, "ctx", "state 2");
      session.setTurnState(3, "ctx", "state 3");

      session.clearStateFromIndex(2);

      expect(session.getTurnState(0, "ctx")).toBe("state 0");
      expect(session.getTurnState(1, "ctx")).toBe("state 1");
      expect(session.getTurnState(2, "ctx")).toBeUndefined();
      expect(session.getTurnState(3, "ctx")).toBeUndefined();
    });

    it("should not clear state before specified index", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.setTurnState(1, "ctx", "state 1");
      session.setTurnState(2, "ctx", "state 2");

      session.clearStateFromIndex(1);

      expect(session.getTurnState(0, "ctx")).toBe("state 0");
      expect(session.getTurnState(1, "ctx")).toBeUndefined();
      expect(session.getTurnState(2, "ctx")).toBeUndefined();
    });
  });

  describe("clearAllStates", () => {
    it("should clear all stored states", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.setTurnState(1, "ctx", "state 1");
      session.setTurnState(2, "ctx", "state 2");

      session.clearAllStates();

      expect(session.getTurnState(0, "ctx")).toBeUndefined();
      expect(session.getTurnState(1, "ctx")).toBeUndefined();
      expect(session.getTurnState(2, "ctx")).toBeUndefined();
    });

    it("should handle clearing when state is empty", () => {
      expect(() => {
        session.clearAllStates();
      }).not.toThrow();
    });
  });

  describe("Integration with Message Operations", () => {
    it("should clear all states on cleanup", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.setTurnState(1, "ctx", "state 1");

      session.cleanup();

      expect(session.getTurnState(0, "ctx")).toBeUndefined();
      expect(session.getTurnState(1, "ctx")).toBeUndefined();
    });
  });

  describe("Legacy API Compatibility", () => {
    it("should support getTurnDynamicContext as alias", () => {
      session.setTurnDynamicContext(0, "legacy context");
      expect(session.getTurnDynamicContext(0)).toBe("legacy context");
      expect(session.getTurnState(0, "dynamicContext")).toBe("legacy context");
    });

    it("should support setTurnDynamicContext as alias", () => {
      session.setTurnState(1, "dynamicContext", "new state");
      expect(session.getTurnDynamicContext(1)).toBe("new state");
    });

    it("should support clearTurnContextFromIndex as alias", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.setTurnState(1, "ctx", "state 1");

      session.clearTurnContextFromIndex(1);

      expect(session.getTurnState(0, "ctx")).toBe("state 0");
      expect(session.getTurnState(1, "ctx")).toBeUndefined();
    });

    it("should support clearAllTurnContexts as alias", () => {
      session.setTurnState(0, "ctx", "state 0");
      session.clearAllTurnContexts();
      expect(session.getTurnState(0, "ctx")).toBeUndefined();
    });
  });

  describe("Persistence & Checkpointing", () => {
    it("should include turn states in getState", () => {
      session.setTurnState(0, "dynamicContext", "test context");
      session.setTurnState(1, "metadata", { id: 123 });

      const state = session.getState();

      expect(state.turnStates).toBeDefined();
      expect(state.turnStates![0]).toEqual({ dynamicContext: "test context" });
      expect(state.turnStates![1]).toEqual({ metadata: { id: 123 } });
    });

    it("should restore turn states from state object", () => {
      // Set initial state
      session.setTurnState(0, "ctx", "original");

      // Create a new state to restore
      const newState = session.getState();
      newState.turnStates = {
        0: { ctx: "restored value" },
        5: { data: "new data" },
      };

      // Restore
      session.restoreState(newState);

      expect(session.getTurnState(0, "ctx")).toBe("restored value");
      expect(session.getTurnState(5, "data")).toBe("new data");
    });

    it("should maintain consistency between messages and turn states during restore", () => {
      session.addUserMessage("hello");
      session.setTurnState(0, "ctx", "context for hello");

      const state = session.getState();

      // Modify messages
      session.addAssistantMessage("hi");

      // Restore to previous state
      session.restoreState(state);

      expect(session.getMessageCount()).toBe(1); // Only the user message
      expect(session.getTurnState(0, "ctx")).toBe("context for hello");
    });
  });
});
