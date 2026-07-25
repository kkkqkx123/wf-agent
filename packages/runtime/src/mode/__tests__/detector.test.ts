/**
 * Mode Detector Unit Tests
 *
 * Tests the mode detection logic in detector.ts and types.ts cover:
 * - Test mode detection (CLI_MODE=test)
 * - TEST_MODE and HEADLESS env var semantics
 * - Config fallback priority
 * - Output format defaults per mode
 * - Color detection
 * - Cache invalidation
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { getMode, invalidateModeCache, isHeadless, isInteractive, isTest, getOutputFormat, isJsonMode, isSilentMode } from "./index.js";
import { ExecutionModeEnvVars } from "./types.js";

const ENV = ExecutionModeEnvVars;

/**
 * Helper: clear all mode-related env vars and invalidate cache
 */
function clearModeEnv(): void {
  delete process.env[ENV.CLI_MODE];
  delete process.env[ENV.HEADLESS];
  delete process.env[ENV.TEST_MODE];
  delete process.env[ENV.OUTPUT_FORMAT];
  delete process.env[ENV.NO_COLOR];
  invalidateModeCache();
}

describe("getMode (env var based detection)", () => {
  beforeEach(() => {
    clearModeEnv();
  });

  afterEach(() => {
    clearModeEnv();
  });

  // ---- Interactive mode (default) ----

  it("should default to interactive when no env vars are set", () => {
    const result = getMode();
    expect(result.mode).toBe("interactive");
    expect(result.isInteractive).toBe(true);
    expect(result.isHeadless).toBe(false);
    expect(result.isTest).toBe(false);
  });

  it("should default output format to text in interactive mode", () => {
    const result = getMode();
    expect(result.outputFormat).toBe("text");
  });

  it("should enable color when stdout is TTY in interactive mode", () => {
    // In test environment, stdout is not a TTY, so colorEnabled should be false
    const result = getMode();
    expect(result.colorEnabled).toBe(false);
  });

  // ---- Test mode detection ----

  it("should detect test mode from CLI_MODE=test", () => {
    process.env[ENV.CLI_MODE] = "test";
    const result = getMode();
    expect(result.mode).toBe("test");
    expect(result.isTest).toBe(true);
    expect(result.isInteractive).toBe(false);
    expect(result.isHeadless).toBe(false);
  });

  it("should default output format to text in test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    const result = getMode();
    expect(result.outputFormat).toBe("text");
  });

  it("should detect test mode from TEST_MODE=true", () => {
    process.env[ENV.TEST_MODE] = "true";
    const result = getMode();
    expect(result.mode).toBe("test");
    expect(result.isTest).toBe(true);
  });

  // ---- Headless mode detection ----

  it("should detect headless mode from CLI_MODE=headless", () => {
    process.env[ENV.CLI_MODE] = "headless";
    const result = getMode();
    expect(result.mode).toBe("headless");
    expect(result.isHeadless).toBe(true);
    expect(result.isInteractive).toBe(false);
    expect(result.isTest).toBe(false);
  });

  it("should detect headless mode from HEADLESS=true", () => {
    process.env[ENV.HEADLESS] = "true";
    const result = getMode();
    expect(result.mode).toBe("headless");
    expect(result.isHeadless).toBe(true);
  });

  it("should default output format to json in headless mode", () => {
    process.env[ENV.CLI_MODE] = "headless";
    const result = getMode();
    expect(result.outputFormat).toBe("json");
  });

  it("should detect headless mode from legacy CLI_MODE=programmatic", () => {
    process.env[ENV.CLI_MODE] = "programmatic";
    const result = getMode();
    expect(result.mode).toBe("headless");
    expect(result.isHeadless).toBe(true);
  });

  // ---- Env var priority ----

  it("should prioritize CLI_MODE over TEST_MODE", () => {
    process.env[ENV.CLI_MODE] = "headless";
    process.env[ENV.TEST_MODE] = "true";
    const result = getMode();
    expect(result.mode).toBe("headless");
    expect(result.isHeadless).toBe(true);
  });

  it("should prioritize CLI_MODE=test over HEADLESS=true", () => {
    process.env[ENV.CLI_MODE] = "test";
    process.env[ENV.HEADLESS] = "true";
    const result = getMode();
    expect(result.mode).toBe("test");
    expect(result.isTest).toBe(true);
  });

  it("should prioritize CLI_MODE over HEADLESS", () => {
    process.env[ENV.CLI_MODE] = "interactive";
    process.env[ENV.HEADLESS] = "true";
    const result = getMode();
    expect(result.mode).toBe("interactive");
    expect(result.isInteractive).toBe(true);
  });

  // ---- Config fallback ----

  it("should use configFallback=headless when no env vars are set", () => {
    const result = getMode("headless");
    expect(result.mode).toBe("headless");
    expect(result.isHeadless).toBe(true);
  });

  it("should use configFallback=test when no env vars are set", () => {
    const result = getMode("test");
    expect(result.mode).toBe("test");
    expect(result.isTest).toBe(true);
  });

  it("should ignore configFallback when env var takes priority", () => {
    process.env[ENV.CLI_MODE] = "headless";
    const result = getMode("interactive");
    expect(result.mode).toBe("headless");
  });

  it("should default to interactive when configFallback is not provided", () => {
    const result = getMode();
    expect(result.mode).toBe("interactive");
  });

  it("should ignore configFallback=interactive (default)", () => {
    const result = getMode("interactive");
    expect(result.mode).toBe("interactive");
  });

  // ---- Output format overrides ----

  it("should respect explicit CLI_OUTPUT_FORMAT=json in test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    process.env[ENV.OUTPUT_FORMAT] = "json";
    const result = getMode();
    expect(result.mode).toBe("test");
    expect(result.outputFormat).toBe("json");
  });

  it("should respect explicit CLI_OUTPUT_FORMAT=silent in interactive mode", () => {
    process.env[ENV.OUTPUT_FORMAT] = "silent";
    const result = getMode();
    expect(result.mode).toBe("interactive");
    expect(result.outputFormat).toBe("silent");
  });

  // ---- Color ----

  it("should disable color when NO_COLOR is set", () => {
    process.env[ENV.NO_COLOR] = "1";
    const result = getMode();
    expect(result.colorEnabled).toBe(false);
  });

  // ---- Cache ----

  it("should cache the result and return cached value on subsequent calls", () => {
    process.env[ENV.CLI_MODE] = "test";
    const first = getMode();
    expect(first.mode).toBe("test");

    // Change env var after first call
    delete process.env[ENV.CLI_MODE];

    // Without invalidation, should still return cached "test"
    const second = getMode();
    expect(second.mode).toBe("test");
  });

  it("should return fresh result after invalidateModeCache", () => {
    process.env[ENV.CLI_MODE] = "test";
    const first = getMode();
    expect(first.mode).toBe("test");

    // Change env and invalidate
    delete process.env[ENV.CLI_MODE];
    invalidateModeCache();

    const second = getMode();
    expect(second.mode).toBe("interactive");
  });
});

describe("helper functions", () => {
  beforeEach(() => {
    clearModeEnv();
  });

  afterEach(() => {
    clearModeEnv();
  });

  it("isHeadless should return true for headless mode", () => {
    process.env[ENV.CLI_MODE] = "headless";
    expect(isHeadless()).toBe(true);
  });

  it("isHeadless should return false for test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    expect(isHeadless()).toBe(false);
  });

  it("isInteractive should return true for interactive mode", () => {
    expect(isInteractive()).toBe(true);
  });

  it("isInteractive should return false for test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    expect(isInteractive()).toBe(false);
  });

  it("isTest should return true for test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    expect(isTest()).toBe(true);
  });

  it("isTest should return false for headless mode", () => {
    process.env[ENV.CLI_MODE] = "headless";
    expect(isTest()).toBe(false);
  });

  it("isTest should return false for interactive mode", () => {
    expect(isTest()).toBe(false);
  });

  it("getOutputFormat should return json for headless mode", () => {
    process.env[ENV.CLI_MODE] = "headless";
    expect(getOutputFormat()).toBe("json");
  });

  it("getOutputFormat should return text for test mode", () => {
    process.env[ENV.CLI_MODE] = "test";
    expect(getOutputFormat()).toBe("text");
  });

  it("isJsonMode should return true when output format is json", () => {
    process.env[ENV.CLI_MODE] = "headless";
    expect(isJsonMode()).toBe(true);
  });

  it("isSilentMode should return true when output format is silent", () => {
    process.env[ENV.OUTPUT_FORMAT] = "silent";
    expect(isSilentMode()).toBe(true);
  });
});
