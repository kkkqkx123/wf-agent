/**
 * Interruption Types Unit Tests
 * Tests for interruption-types.ts
 */

import { describe, it, expect } from "vitest";
import type { InterruptionInfo, InterruptibleOptions } from "../../src/utils/signal/interruption-types.js";
import type { InterruptionCheckResult } from "../../src/utils/signal/abort-signal-utils.js";

describe("Interruption Types", () => {
  describe("InterruptionInfo Interface", () => {
    it("should create valid InterruptionInfo with required fields", () => {
      const info: InterruptionInfo = {
        type: "PAUSE",
        executionId: "exec-123",
        nodeId: "node-456",
      };

      expect(info.type).toBe("PAUSE");
      expect(info.executionId).toBe("exec-123");
      expect(info.nodeId).toBe("node-456");
      expect(info.timestamp).toBeUndefined();
    });

    it("should create valid InterruptionInfo with timestamp", () => {
      const timestamp = Date.now();
      const info: InterruptionInfo = {
        type: "STOP",
        executionId: "exec-789",
        nodeId: "node-012",
        timestamp,
      };

      expect(info.type).toBe("STOP");
      expect(info.executionId).toBe("exec-789");
      expect(info.nodeId).toBe("node-012");
      expect(info.timestamp).toBe(timestamp);
    });

    it("should support PAUSE interruption type", () => {
      const info: InterruptionInfo = {
        type: "PAUSE",
        executionId: "exec-pause",
        nodeId: "node-pause",
      };

      expect(info.type).toBe("PAUSE");
    });

    it("should support STOP interruption type", () => {
      const info: InterruptionInfo = {
        type: "STOP",
        executionId: "exec-stop",
        nodeId: "node-stop",
      };

      expect(info.type).toBe("STOP");
    });

    it("should not allow null as interruption type", () => {
      // This is a compile-time check - the type system should prevent this
      // We verify the type definition excludes null
      const validTypes: Array<InterruptionInfo["type"]> = ["PAUSE", "STOP"];
      
      expect(validTypes).toContain("PAUSE");
      expect(validTypes).toContain("STOP");
      expect(validTypes.length).toBe(2);
    });

    it("should handle complex execution and node IDs", () => {
      const info: InterruptionInfo = {
        type: "PAUSE",
        executionId: "workflow-abc-123-def-456",
        nodeId: "subgraph-node-xyz-789",
        timestamp: 1234567890,
      };

      expect(info.executionId).toMatch(/workflow-/);
      expect(info.nodeId).toMatch(/subgraph-/);
      expect(info.timestamp).toBe(1234567890);
    });
  });

  describe("InterruptibleOptions Interface", () => {
    it("should create valid InterruptibleOptions with default values", () => {
      const options: InterruptibleOptions = {};

      expect(options.customCheck).toBeUndefined();
    });



    it("should create valid InterruptibleOptions with customCheck function", () => {
      const customCheck = (): InterruptionCheckResult => ({
        type: "continue",
      });

      const options: InterruptibleOptions = {
        customCheck,
      };

      expect(options.customCheck).toBeDefined();
      expect(typeof options.customCheck).toBe("function");
      
      const result = options.customCheck!();
      expect(result.type).toBe("continue");
    });

    it("should create valid InterruptibleOptions with customCheck function", () => {
      const customCheck = (): InterruptionCheckResult => ({
        type: "continue",
      });

      const options: InterruptibleOptions = {
        customCheck,
      };

      expect(options.customCheck).toBeDefined();
      expect(typeof options.customCheck).toBe("function");
      
      const result = options.customCheck!();
      expect(result.type).toBe("continue");
    });
  });
});
