/**
 * Tool Permission Manager Tests
 */

import { describe, it, expect } from "vitest";
import { ToolPermissionManager } from "../tool-permission-manager.js";

describe("ToolPermissionManager", () => {
  const allTools = ["read_file", "write_file", "edit_file", "delete_file", "run_shell"];

  describe("initialization", () => {
    it("should initialize with specified enabled tools", () => {
      const manager = new ToolPermissionManager(["read_file", "write_file"], allTools);

      expect(manager.isEnabled("read_file")).toBe(true);
      expect(manager.isEnabled("write_file")).toBe(true);
      expect(manager.isDisabled("edit_file")).toBe(true);
      expect(manager.isDisabled("delete_file")).toBe(true);
    });

    it("should enable all tools if initial list is empty", () => {
      const manager = new ToolPermissionManager([], allTools);

      allTools.forEach(tool => {
        expect(manager.isEnabled(tool)).toBe(true);
      });
    });
  });

  describe("enable/disable operations", () => {
    it("should disable tools", () => {
      const manager = new ToolPermissionManager(allTools, allTools);

      manager.disableTools(["write_file", "edit_file"], "Testing");

      expect(manager.isDisabled("write_file")).toBe(true);
      expect(manager.isDisabled("edit_file")).toBe(true);
      expect(manager.isEnabled("read_file")).toBe(true);
    });

    it("should enable previously disabled tools", () => {
      const manager = new ToolPermissionManager(["read_file"], allTools);

      manager.enableTools(["write_file"]);

      expect(manager.isEnabled("write_file")).toBe(true);
      expect(manager.isDisabled("write_file")).toBe(false);
    });

    it("should ignore tools not in schema", () => {
      const manager = new ToolPermissionManager(["read_file"], allTools);

      manager.enableTools(["nonexistent_tool"]);

      expect(manager.getEnabledTools()).toEqual(["read_file"]);
    });
  });

  describe("state tracking", () => {
    it("should track permission change history", () => {
      const manager = new ToolPermissionManager(allTools, allTools);

      manager.disableTools(["write_file"], "Test reason");
      manager.enableTools(["write_file"]);

      const state = manager.getState();
      expect(state.history.length).toBe(2);
      expect(state.history[0]?.type).toBe("disable");
      expect(state.history[1]?.type).toBe("enable");
    });

    it("should get block reason", () => {
      const manager = new ToolPermissionManager(allTools, allTools);

      manager.disableTools(["write_file"], "Custom reason");

      expect(manager.getBlockReason("write_file")).toBe("Custom reason");
    });
  });

  describe("serialization", () => {
    it("should serialize and deserialize state", () => {
      const original = new ToolPermissionManager(allTools, allTools);
      original.disableTools(["edit_file"], "Test");

      const serialized = original.serialize();
      const restored = ToolPermissionManager.deserialize(serialized, allTools);

      expect(restored.isEnabled("read_file")).toBe(true);
      expect(restored.isEnabled("write_file")).toBe(true);
      expect(restored.isDisabled("edit_file")).toBe(true);
      expect(restored.getState().history.length).toBeGreaterThan(0);
    });
  });
});
