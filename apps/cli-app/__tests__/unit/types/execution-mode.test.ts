/**
 * Execution Mode Types Unit Tests
 */

import { describe, it, expect } from "vitest";
import {
  defaultExitCodes,
  defaultExecutionConfig,
  ExecutionModeEnvVars,
} from "../../../src/types/execution-mode.js";

describe("Execution Mode Types", () => {
  describe("defaultExitCodes", () => {
    it("should have correct exit codes", () => {
      expect(defaultExitCodes.success).toBe(0);
      expect(defaultExitCodes.error).toBe(1);
      expect(defaultExitCodes.validationError).toBe(2);
      expect(defaultExitCodes.timeout).toBe(124);
      expect(defaultExitCodes.cancelled).toBe(130);
    });
  });

  describe("defaultExecutionConfig", () => {
    it("should have correct default values", () => {
      expect(defaultExecutionConfig.mode).toBe("interactive");
      expect(defaultExecutionConfig.outputFormat).toBe("text");
      expect(defaultExecutionConfig.autoExit).toBe(false);
      expect(defaultExecutionConfig.timeout).toBe(30000);
      expect(defaultExecutionConfig.logLevel).toBe("info");
      expect(defaultExecutionConfig.noColor).toBe(false);
    });

    it("should include exit codes", () => {
      expect(defaultExecutionConfig.exitCodes).toEqual(defaultExitCodes);
    });
  });

  describe("ExecutionModeEnvVars", () => {
    it("should define all environment variables", () => {
      expect(ExecutionModeEnvVars.CLI_MODE).toBe("CLI_MODE");
      expect(ExecutionModeEnvVars.HEADLESS).toBe("HEADLESS");
      expect(ExecutionModeEnvVars.TEST_MODE).toBe("TEST_MODE");
      expect(ExecutionModeEnvVars.OUTPUT_FORMAT).toBe("CLI_OUTPUT_FORMAT");
      expect(ExecutionModeEnvVars.LOG_LEVEL).toBe("CLI_LOG_LEVEL");
      expect(ExecutionModeEnvVars.LOG_FILE).toBe("CLI_LOG_FILE");
      expect(ExecutionModeEnvVars.NO_COLOR).toBe("NO_COLOR");
    });
  });
});
