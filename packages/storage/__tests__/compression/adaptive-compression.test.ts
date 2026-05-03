/**
 * Unit tests for adaptive compression utilities
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  detectDataType,
  selectCompressionStrategy,
  logCompressionDecision,
} from "../../src/compression/adaptive-compression.js";
import type { DataType } from "../../src/compression/adaptive-compression.js";

describe("detectDataType", () => {
  it("should return 'unknown' for empty data", () => {
    const data = new Uint8Array(0);
    expect(detectDataType(data)).toBe("unknown");
  });

  it("should detect JSON starting with object brace", () => {
    const jsonStr = '{"key": "value"}';
    const data = new TextEncoder().encode(jsonStr);
    expect(detectDataType(data)).toBe("json");
  });

  it("should detect JSON starting with array brace", () => {
    const jsonStr = '[{"key": "value"}]';
    const data = new TextEncoder().encode(jsonStr);
    expect(detectDataType(data)).toBe("json");
  });

  it("should detect JSON with leading whitespace", () => {
    const jsonStr = '   {"key": "value"}';
    const data = new TextEncoder().encode(jsonStr);
    expect(detectDataType(data)).toBe("json");
  });

  it("should detect JSON with newline prefix", () => {
    const jsonStr = '\n\n{"key": "value"}';
    const data = new TextEncoder().encode(jsonStr);
    expect(detectDataType(data)).toBe("json");
  });

  it("should detect text data with high printable ratio", () => {
    const text = "This is a plain text document with lots of readable content.";
    const data = new TextEncoder().encode(text);
    expect(detectDataType(data)).toBe("text");
  });

  it("should detect text data with tabs and newlines", () => {
    const text = "Line 1\nLine 2\tTabbed\nLine 3";
    const data = new TextEncoder().encode(text);
    expect(detectDataType(data)).toBe("text");
  });

  it("should detect binary data with low printable ratio", () => {
    // Create binary data with many non-printable bytes
    const data = new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    expect(detectDataType(data)).toBe("binary");
  });

  it("should detect binary data with mixed content", () => {
    // Mix of printable and non-printable (less than 90% printable)
    const data = new Uint8Array(100);
    for (let i = 0; i < 100; i++) {
      data[i] = i < 85 ? 32 + (i % 95) : Math.floor(Math.random() * 32);
    }
    expect(detectDataType(data)).toBe("binary");
  });

  it("should handle large text data efficiently", () => {
    const text = "A".repeat(2000);
    const data = new TextEncoder().encode(text);
    expect(detectDataType(data)).toBe("text");
  });

  it("should only check first 100 bytes for JSON detection", () => {
    // Create data with whitespace followed by JSON after position 100
    const padding = new Uint8Array(100).fill(32); // spaces
    const jsonPart = new TextEncoder().encode('{"test": true}');
    const combined = new Uint8Array(padding.length + jsonPart.length);
    combined.set(padding);
    combined.set(jsonPart, padding.length);
    
    // Should not detect as JSON because first 100 bytes are all spaces
    const result = detectDataType(combined);
    expect(result).toBe("text"); // Falls back to text detection
  });
});

describe("selectCompressionStrategy", () => {
  it("should disable compression for small data (< 100 bytes)", () => {
    const data = new Uint8Array(50);
    const config = selectCompressionStrategy(data);
    expect(config.enabled).toBe(false);
  });

  it("should use brotli for large JSON (> 10KB)", () => {
    const jsonStr = JSON.stringify({ data: "x".repeat(11 * 1024) });
    const data = new TextEncoder().encode(jsonStr);
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("brotli");
    expect(config.threshold).toBe(0);
    expect(config.level).toBe(8);
  });

  it("should use gzip for small JSON (100 bytes - 10KB)", () => {
    const jsonStr = JSON.stringify({ data: "x".repeat(500) });
    const data = new TextEncoder().encode(jsonStr);
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("gzip");
    expect(config.threshold).toBe(0);
    expect(config.level).toBe(6);
  });

  it("should use brotli for large text (> 50KB)", () => {
    const text = "A".repeat(51 * 1024);
    const data = new TextEncoder().encode(text);
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("brotli");
    expect(config.threshold).toBe(0);
    expect(config.level).toBe(7);
  });

  it("should use gzip for small text (100 bytes - 50KB)", () => {
    const text = "A".repeat(1000);
    const data = new TextEncoder().encode(text);
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("gzip");
    expect(config.threshold).toBe(0);
    expect(config.level).toBe(6);
  });

  it("should use gzip with threshold for large binary (> 100KB)", () => {
    const data = new Uint8Array(101 * 1024);
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("gzip");
    expect(config.threshold).toBe(10 * 1024);
    expect(config.level).toBe(6);
  });

  it("should use gzip with small threshold for small binary (100 bytes - 100KB)", () => {
    const data = new Uint8Array(50 * 1024);
    // Make it binary by adding non-printable bytes
    for (let i = 0; i < 100; i++) {
      data[i] = i % 32;
    }
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("gzip");
    expect(config.threshold).toBe(1024);
    expect(config.level).toBe(6);
  });

  it("should use default config for unknown data types", () => {
    const data = new Uint8Array(200);
    // Fill with mix that doesn't clearly match any type
    for (let i = 0; i < 200; i++) {
      data[i] = 127 + (i % 128); // Non-ASCII range
    }
    const config = selectCompressionStrategy(data);
    
    expect(config.enabled).toBe(true);
    expect(config.algorithm).toBe("gzip");
    expect(config.threshold).toBe(1024);
    expect(config.level).toBe(6);
  });
});

describe("logCompressionDecision", () => {
  let stderrWriteSpy: any;

  beforeEach(() => {
    stderrWriteSpy = vi.spyOn(process.stderr, "write").mockImplementation(() => true);
  });

  afterEach(() => {
    stderrWriteSpy.mockRestore();
  });

  it("should log compression decision when algorithm is used", () => {
    logCompressionDecision(1000, 500, "gzip", "json");
    
    expect(stderrWriteSpy).toHaveBeenCalled();
    const loggedMessage = stderrWriteSpy.mock.calls[0][0];
    expect(loggedMessage).toContain("[Compression]");
    expect(loggedMessage).toContain("Type: json");
    expect(loggedMessage).toContain("Algorithm: gzip");
    expect(loggedMessage).toContain("Original: 1000B");
    expect(loggedMessage).toContain("Compressed: 500B");
    expect(loggedMessage).toContain("Savings: 50.0%");
  });

  it("should log skipped compression when no algorithm", () => {
    logCompressionDecision(100, 100, null, "binary");
    
    expect(stderrWriteSpy).toHaveBeenCalled();
    const loggedMessage = stderrWriteSpy.mock.calls[0][0];
    expect(loggedMessage).toContain("[Compression]");
    expect(loggedMessage).toContain("Type: binary");
    expect(loggedMessage).toContain("Skipped (no benefit)");
    expect(loggedMessage).toContain("Size: 100B");
  });

  it("should calculate correct savings percentage", () => {
    logCompressionDecision(1000, 250, "brotli", "text");
    
    const loggedMessage = stderrWriteSpy.mock.calls[0][0];
    expect(loggedMessage).toContain("Savings: 75.0%");
  });

  it("should handle zero savings when compression ratio is 1:1", () => {
    // When algorithm is provided but sizes are equal
    logCompressionDecision(1000, 1000, "gzip", "binary");
    
    const loggedMessage = stderrWriteSpy.mock.calls[0][0];
    expect(loggedMessage).toContain("Savings: 0.0%");
  });

  it("should handle negative savings (expansion)", () => {
    logCompressionDecision(100, 150, "gzip", "binary");
    
    const loggedMessage = stderrWriteSpy.mock.calls[0][0];
    expect(loggedMessage).toContain("Savings: -50.0%");
  });
});
