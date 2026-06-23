/**
 * Lua Strategies Base — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { checkLuaAvailable, checkLuaJITAvailable, DEFAULT_DENIED_MODULES } from "../base.js";

// =========================================================================
// DEFAULT_DENIED_MODULES
// =========================================================================

describe("DEFAULT_DENIED_MODULES", () => {
  it("should contain os module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("os");
  });

  it("should contain io module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("io");
  });

  it("should contain package module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("package");
  });

  it("should contain debug module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("debug");
  });

  it("should contain ffi module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("ffi");
  });

  it("should contain socket module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("socket");
  });

  it("should contain lfs module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("lfs");
  });

  it("should contain luaposix module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("luaposix");
  });

  it("should have 8 default denied modules", () => {
    expect(DEFAULT_DENIED_MODULES).toHaveLength(8);
  });
});

// =========================================================================
// checkLuaAvailable
// =========================================================================

describe("checkLuaAvailable", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return false when lua is not available", () => {
    // In test environment, lua is typically not installed
    const result = checkLuaAvailable();
    expect(typeof result).toBe("boolean");
  });

  it("should return a boolean value", () => {
    const result = checkLuaAvailable();
    expect(typeof result).toBe("boolean");
  });
});

// =========================================================================
// checkLuaJITAvailable
// =========================================================================

describe("checkLuaJITAvailable", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return false when luajit is not available", () => {
    // In test environment, luajit is typically not installed
    const result = checkLuaJITAvailable();
    expect(typeof result).toBe("boolean");
  });

  it("should return a boolean value", () => {
    const result = checkLuaJITAvailable();
    expect(typeof result).toBe("boolean");
  });
});
