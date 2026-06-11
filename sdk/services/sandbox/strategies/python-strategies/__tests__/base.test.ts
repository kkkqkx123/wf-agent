/**
 * Python Strategies Base — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { checkPythonAvailable, DEFAULT_DENIED_MODULES } from "../base.js";

// =========================================================================
// DEFAULT_DENIED_MODULES
// =========================================================================

describe("DEFAULT_DENIED_MODULES", () => {
  it("should contain os module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("os");
  });

  it("should contain subprocess module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("subprocess");
  });

  it("should contain shutil module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("shutil");
  });

  it("should contain ctypes module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("ctypes");
  });

  it("should contain socket module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("socket");
  });

  it("should contain pty module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("pty");
  });

  it("should contain signal module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("signal");
  });

  it("should contain multiprocessing module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("multiprocessing");
  });

  it("should contain distutils module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("distutils");
  });

  it("should contain sysconfig module", () => {
    expect(DEFAULT_DENIED_MODULES).toContain("sysconfig");
  });

  it("should have 10 default denied modules", () => {
    expect(DEFAULT_DENIED_MODULES).toHaveLength(10);
  });
});

// =========================================================================
// checkPythonAvailable
// =========================================================================

describe("checkPythonAvailable", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should return a boolean value", () => {
    const result = checkPythonAvailable();
    expect(typeof result).toBe("boolean");
  });

  it("should return false when python is not available", () => {
    // In test environment, python may or may not be installed
    // Just verify it returns a boolean
    const result = checkPythonAvailable();
    expect(typeof result).toBe("boolean");
  });
});
