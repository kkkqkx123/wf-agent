import { describe, it, expect } from "vitest";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../constants.js";

describe("DEFAULT_DELTA_STORAGE_CONFIG", () => {
  it("should have enabled set to true", () => {
    expect(DEFAULT_DELTA_STORAGE_CONFIG.enabled).toBe(true);
  });

  it("should have baselineInterval set to 10", () => {
    expect(DEFAULT_DELTA_STORAGE_CONFIG.baselineInterval).toBe(10);
  });

  it("should have maxDeltaChainLength set to 20", () => {
    expect(DEFAULT_DELTA_STORAGE_CONFIG.maxDeltaChainLength).toBe(20);
  });
});
