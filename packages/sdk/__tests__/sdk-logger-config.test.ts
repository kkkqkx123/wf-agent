/**
 * SDK Logger Configuration Tests
 * Tests the configureSDKLogger function directly without full SDK initialization
 */

import { describe, it, expect, afterEach } from "vitest";
import { configureSDKLogger } from "../utils/logger.js";
import { createConsoleStream } from "@wf-agent/common-utils";

describe("SDK Logger Configuration", () => {
  afterEach(() => {
    // Reset logger state between tests
    // Note: In a real scenario, we'd need a reset function
  });

  describe("configureSDKLogger", () => {
    it("should accept simplified config with level only", () => {
      expect(() => {
        configureSDKLogger({
          level: "debug",
        });
      }).not.toThrow();
    });

    it("should accept config with level and stream", () => {
      const stream = createConsoleStream({ json: false, timestamp: true });

      expect(() => {
        configureSDKLogger({
          level: "info",
          stream,
        });
      }).not.toThrow();
    });

    it("should accept minimal config", () => {
      expect(() => {
        configureSDKLogger({});
      }).not.toThrow();
    });

    it("should support all log levels", () => {
      const levels: Array<"debug" | "info" | "warn" | "error"> = ["debug", "info", "warn", "error"];

      levels.forEach(level => {
        expect(() => {
          configureSDKLogger({ level });
        }).not.toThrow();
      });
    });
  });
});
