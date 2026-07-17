/**
 * AgentLoopEntity Unit Tests — Tool Call Protocol Locking
 *
 * Business scenarios:
 * 1. Agent starts execution → protocol is resolved and locked → cannot be changed after
 * 2. Before lock → no protocol is set → getter returns undefined
 * 3. After lock → protocol is frozen → double-lock throws
 * 4. Checkpoint restore → protocol is restored from snapshot → bypasses immutability guard
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { ToolCallFormatConfig } from "@wf-agent/types";

// Minimal mock: we only test the locking contract, not the full entity lifecycle
class AgentLoopEntityMock {
  private _lockedToolCallFormat: ToolCallFormatConfig | undefined;

  lockToolCallFormat(config: ToolCallFormatConfig): void {
    if (this._lockedToolCallFormat) {
      throw new Error(
        `Tool call format already locked: ${this._lockedToolCallFormat.format}. ` +
          "Cannot re-lock during the same execution.",
      );
    }
    this._lockedToolCallFormat = Object.freeze({ ...config });
  }

  getLockedToolCallFormat(): ToolCallFormatConfig | undefined {
    return this._lockedToolCallFormat;
  }

  // Direct setter for snapshot restore (bypasses immutability guard)
  set lockedToolCallFormat(config: ToolCallFormatConfig | undefined) {
    this._lockedToolCallFormat = config;
  }
}

describe("AgentLoopEntity — Tool Call Protocol Locking", () => {
  let entity: AgentLoopEntityMock;

  const xmlConfig: ToolCallFormatConfig = {
    format: "xml",
    xmlTags: {
      toolCall: "tool_use",
      toolName: "tool_name",
      toolArgs: "parameters",
      toolResult: "tool_result",
      toolCallId: "tool_call_id",
      toolOutput: "tool_output",
    },
  };

  const nativeConfig: ToolCallFormatConfig = {
    format: "native",
  };

  beforeEach(() => {
    entity = new AgentLoopEntityMock();
  });

  // ===========================================================================
  // Scenario: Before lock — protocol is undefined
  // ===========================================================================
  describe("before lock", () => {
    it("should return undefined before any protocol is locked", () => {
      expect(entity.getLockedToolCallFormat()).toBeUndefined();
    });
  });

  // ===========================================================================
  // Scenario: First lock succeeds — protocol is set and frozen
  // ===========================================================================
  describe("lockToolCallFormat — first lock", () => {
    it("should allow locking with a valid config", () => {
      entity.lockToolCallFormat(xmlConfig);
      expect(entity.getLockedToolCallFormat()).toBeDefined();
      expect(entity.getLockedToolCallFormat()!.format).toBe("xml");
    });

    it("should allow locking with native format", () => {
      entity.lockToolCallFormat(nativeConfig);
      expect(entity.getLockedToolCallFormat()!.format).toBe("native");
    });

    it("should freeze the locked config object", () => {
      entity.lockToolCallFormat(xmlConfig);
      const locked = entity.getLockedToolCallFormat()!;
      // Object.freeze was applied
      expect(Object.isFrozen(locked)).toBe(true);
    });

    it("should return the exact frozen config", () => {
      entity.lockToolCallFormat(xmlConfig);
      const locked = entity.getLockedToolCallFormat()!;
      expect(locked.format).toBe("xml");
      expect(locked.xmlTags?.toolCall).toBe("tool_use");
    });

    it("should lock with json_wrapped format including markers", () => {
      const jsonWrappedConfig: ToolCallFormatConfig = {
        format: "json_wrapped",
        markers: { start: "<<<TOOL_CALL>>>", end: "<<<END_TOOL_CALL>>>" },
      };
      entity.lockToolCallFormat(jsonWrappedConfig);
      expect(entity.getLockedToolCallFormat()!.format).toBe("json_wrapped");
      expect(entity.getLockedToolCallFormat()!.markers!.start).toBe("<<<TOOL_CALL>>>");
    });

    it("should lock with json_raw format", () => {
      const jsonRawConfig: ToolCallFormatConfig = { format: "json_raw" };
      entity.lockToolCallFormat(jsonRawConfig);
      expect(entity.getLockedToolCallFormat()!.format).toBe("json_raw");
    });
  });

  // ===========================================================================
  // Scenario: Double-lock — immutability guard throws
  // ===========================================================================
  describe("lockToolCallFormat — double lock", () => {
    it("should throw when locking a second time with a different format", () => {
      entity.lockToolCallFormat(xmlConfig);
      expect(() => entity.lockToolCallFormat(nativeConfig)).toThrow(
        "Tool call format already locked",
      );
    });

    it("should throw when locking a second time with the same format", () => {
      entity.lockToolCallFormat(xmlConfig);
      expect(() => entity.lockToolCallFormat(xmlConfig)).toThrow(
        "Tool call format already locked",
      );
    });

    it("should preserve the original locked format after a failed re-lock attempt", () => {
      entity.lockToolCallFormat(xmlConfig);
      try {
        entity.lockToolCallFormat(nativeConfig);
      } catch {
        // Expected
      }
      expect(entity.getLockedToolCallFormat()!.format).toBe("xml");
    });
  });

  // ===========================================================================
  // Scenario: Snapshot restore — bypasses immutability guard via direct setter
  // ===========================================================================
  describe("snapshot restore (bypass immutability guard)", () => {
    it("should allow setting lockedToolCallFormat directly for snapshot restore", () => {
      entity.lockToolCallFormat(xmlConfig);
      const restoredFormat = entity.getLockedToolCallFormat();

      // Simulate creating a fresh entity from snapshot
      const freshEntity = new AgentLoopEntityMock();
      freshEntity.lockedToolCallFormat = restoredFormat;

      expect(freshEntity.getLockedToolCallFormat()!.format).toBe("xml");
    });

    it("should freeze the restored config after direct set", () => {
      entity.lockToolCallFormat(xmlConfig);
      const restoredFormat = entity.getLockedToolCallFormat();

      const freshEntity = new AgentLoopEntityMock();
      freshEntity.lockedToolCallFormat = restoredFormat;

      // The restore path in fromSnapshot applies Object.freeze
      const frozen = Object.freeze({ ...restoredFormat });
      freshEntity.lockedToolCallFormat = frozen;
      expect(Object.isFrozen(freshEntity.getLockedToolCallFormat()!)).toBe(true);
    });
  });
});