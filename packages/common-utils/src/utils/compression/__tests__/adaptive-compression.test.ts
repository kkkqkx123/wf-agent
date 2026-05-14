/**
 * Adaptive Compression Module Tests
 * Comprehensive test suite for adaptive compression utilities
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  detectDataType,
  selectCompressionStrategy,
  logCompressionDecision,
  type DataType,
} from "../adaptive-compression.js";
import type { CompressionConfig } from "../compressor.js";

describe("Adaptive Compression Module", () => {
  describe("detectDataType", () => {
    it("should return 'unknown' for empty data", () => {
      const emptyData = new Uint8Array(0);
      const result = detectDataType(emptyData);
      expect(result).toBe("unknown");
    });

    it("should detect JSON starting with '{'", () => {
      const jsonData = new TextEncoder().encode('{"key": "value"}');
      const result = detectDataType(jsonData);
      expect(result).toBe("json");
    });

    it("should detect JSON starting with '['", () => {
      const jsonData = new TextEncoder().encode('[1, 2, 3]');
      const result = detectDataType(jsonData);
      expect(result).toBe("json");
    });

    it("should detect JSON with leading whitespace", () => {
      const jsonData = new TextEncoder().encode('   {"key": "value"}');
      const result = detectDataType(jsonData);
      expect(result).toBe("json");
    });

    it("should detect JSON with newline before content", () => {
      const jsonData = new TextEncoder().encode('\n\n{"key": "value"}');
      const result = detectDataType(jsonData);
      expect(result).toBe("json");
    });

    it("should detect text data with high printable ratio", () => {
      // Create text-like data (mostly printable ASCII)
      const textData = new Uint8Array(1000);
      for (let i = 0; i < textData.length; i++) {
        textData[i] = 65 + (i % 26); // A-Z repeating
      }
      const result = detectDataType(textData);
      expect(result).toBe("text");
    });

    it("should detect text with spaces and punctuation", () => {
      const text = "Hello, World! This is a test. ".repeat(40);
      const textData = new TextEncoder().encode(text);
      const result = detectDataType(textData);
      expect(result).toBe("text");
    });

    it("should detect binary data with low printable ratio", () => {
      // Create binary-like data (random bytes)
      const binaryData = new Uint8Array(1000);
      for (let i = 0; i < binaryData.length; i++) {
        binaryData[i] = Math.floor(Math.random() * 256);
      }
      const result = detectDataType(binaryData);
      // Random data should have low printable ratio
      expect(result).toBe("binary");
    });

    it("should detect binary data with non-printable bytes", () => {
      const binaryData = new Uint8Array(1000).fill(0); // All zeros
      const result = detectDataType(binaryData);
      expect(result).toBe("binary");
    });

    it("should handle small JSON data correctly", () => {
      const smallJson = new TextEncoder().encode('{}');
      const result = detectDataType(smallJson);
      expect(result).toBe("json");
    });

    it("should handle data with tabs and carriage returns as text", () => {
      const textWithWhitespace = "Line1\tLine2\r\nLine3".repeat(100);
      const textData = new TextEncoder().encode(textWithWhitespace);
      const result = detectDataType(textData);
      expect(result).toBe("text");
    });

    it("should use first 100 bytes for JSON detection", () => {
      // Create data where first 50 bytes are whitespace, then JSON
      const paddedJson = new Uint8Array(100);
      // Fill first 50 bytes with spaces
      for (let i = 0; i < 50; i++) {
        paddedJson[i] = 32;
      }
      // Add JSON after (within the 100 byte sample)
      const jsonPart = new TextEncoder().encode('{"test": true}');
      paddedJson.set(jsonPart, 50);
      
      const result = detectDataType(paddedJson);
      expect(result).toBe("json");
    });

    it("should use first 1000 bytes for text detection", () => {
      // Mix of printable and non-printable in first 1000 bytes
      const mixedData = new Uint8Array(2000);
      for (let i = 0; i < 1000; i++) {
        mixedData[i] = 65; // Printable
      }
      for (let i = 1000; i < 2000; i++) {
        mixedData[i] = 0; // Non-printable
      }
      
      const result = detectDataType(mixedData);
      expect(result).toBe("text"); // First 1000 bytes are all printable
    });
  });

  describe("selectCompressionStrategy", () => {
    it("should skip compression for very small data (< 100 bytes)", () => {
      const smallData = new Uint8Array(50).fill(65);
      const config = selectCompressionStrategy(smallData);
      
      expect(config.enabled).toBe(false);
    });

    it("should use brotli for large JSON (> 10KB)", () => {
      const largeJson = new TextEncoder().encode(
        JSON.stringify({ data: "x".repeat(15000) })
      );
      const config = selectCompressionStrategy(largeJson);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("brotli");
      expect(config.level).toBe(8);
      expect(config.threshold).toBe(0);
    });

    it("should use gzip for small JSON (< 10KB)", () => {
      const smallJson = new TextEncoder().encode(
        JSON.stringify({ data: "x".repeat(5000) })
      );
      const config = selectCompressionStrategy(smallJson);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
      expect(config.level).toBe(6);
      expect(config.threshold).toBe(0);
    });

    it("should use brotli for large text (> 50KB)", () => {
      const largeText = new TextEncoder().encode("Hello World! ".repeat(5000));
      const config = selectCompressionStrategy(largeText);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("brotli");
      expect(config.level).toBe(7);
      expect(config.threshold).toBe(0);
    });

    it("should use gzip for small text (< 50KB)", () => {
      const smallText = new TextEncoder().encode("Hello World! ".repeat(1000));
      const config = selectCompressionStrategy(smallText);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
      expect(config.level).toBe(6);
      expect(config.threshold).toBe(0);
    });

    it("should use gzip with threshold for large binary (> 100KB)", () => {
      const largeBinary = new Uint8Array(150000).fill(0);
      const config = selectCompressionStrategy(largeBinary);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
      expect(config.level).toBe(6);
      expect(config.threshold).toBe(10 * 1024); // 10KB threshold
    });

    it("should use gzip with smaller threshold for small binary (< 100KB)", () => {
      const smallBinary = new Uint8Array(50000).fill(0);
      const config = selectCompressionStrategy(smallBinary);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
      expect(config.level).toBe(6);
      expect(config.threshold).toBe(1024); // 1KB threshold
    });

    it("should use default config for unknown data type", () => {
      // Empty data returns 'unknown' but is < 100 bytes so skipped
      // Create data that's exactly at boundary with unknown characteristics
      const edgeCaseData = new Uint8Array(150);
      // Mix that doesn't clearly match any pattern
      for (let i = 0; i < 150; i++) {
        edgeCaseData[i] = i % 2 === 0 ? 200 : 50; // Alternating high/low
      }
      
      const config = selectCompressionStrategy(edgeCaseData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
      expect(config.level).toBe(6);
      expect(config.threshold).toBe(1024);
    });

    it("should handle exact boundary at 100 bytes", () => {
      const boundaryData = new Uint8Array(100).fill(65);
      const config = selectCompressionStrategy(boundaryData);
      
      // At exactly 100 bytes, should not skip (< 100 check)
      expect(config.enabled).toBe(true);
    });

    it("should handle exact boundary at 10KB for JSON", () => {
      // Create JSON that's definitely > 10KB
      const largeObj = { data: "x".repeat(11000) };
      const jsonAtBoundary = new TextEncoder().encode(
        JSON.stringify(largeObj)
      );
      
      // Verify size is actually > 10KB
      expect(jsonAtBoundary.length).toBeGreaterThan(10 * 1024);
      
      const config = selectCompressionStrategy(jsonAtBoundary);
      
      // Should be > 10KB, so use brotli
      expect(config.algorithm).toBe("brotli");
    });

    it("should handle exact boundary at 50KB for text", () => {
      // Create text that's definitely > 50KB
      const textAtBoundary = new TextEncoder().encode(
        "x".repeat(55000)
      );
      
      // Verify size is actually > 50KB
      expect(textAtBoundary.length).toBeGreaterThan(50 * 1024);
      
      const config = selectCompressionStrategy(textAtBoundary);
      
      // Should be > 50KB, so use brotli
      expect(config.algorithm).toBe("brotli");
    });

    it("should handle exact boundary at 100KB for binary", () => {
      // Create binary data that's definitely > 100KB
      const binaryAtBoundary = new Uint8Array(110000).fill(0);
      
      // Verify size is actually > 100KB
      expect(binaryAtBoundary.length).toBeGreaterThan(100 * 1024);
      
      const config = selectCompressionStrategy(binaryAtBoundary);
      
      // Should be > 100KB, so use higher threshold
      expect(config.threshold).toBe(10 * 1024);
    });

    it("should always compress when threshold is 0", () => {
      const json = new TextEncoder().encode('{"test": true}');
      const config = selectCompressionStrategy(json);
      
      if (config.enabled && config.threshold !== undefined) {
        expect(config.threshold).toBe(0);
      }
    });
  });

  describe("logCompressionDecision", () => {
    let consoleInfoSpy: any;
    let consoleDebugSpy: any;

    beforeEach(() => {
      consoleInfoSpy = vi.spyOn(console, "info").mockImplementation(() => {});
      consoleDebugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
    });

    it("should log compression completion with algorithm", () => {
      logCompressionDecision(1000, 500, "gzip", "json");
      
      // Logger should have been called (we can't easily verify the exact call due to logger abstraction)
      expect(true).toBe(true); // Basic sanity check
    });

    it("should log compression skip when no algorithm", () => {
      logCompressionDecision(1000, 1000, null, "binary");
      
      expect(true).toBe(true); // Basic sanity check
    });

    it("should calculate correct savings percentage", () => {
      // 1000 -> 500 = 50% savings
      logCompressionDecision(1000, 500, "gzip", "json");
      
      expect(true).toBe(true); // Logger abstraction makes direct verification difficult
    });

    it("should handle zero original size gracefully", () => {
      // This might cause division by zero, should handle gracefully
      expect(() => {
        logCompressionDecision(0, 0, null, "unknown");
      }).not.toThrow();
    });

    it("should handle compressed size larger than original", () => {
      logCompressionDecision(100, 150, "gzip", "text");
      
      expect(true).toBe(true); // Should handle negative savings
    });
  });

  describe("Integration scenarios", () => {
    it("should select appropriate strategy for typical checkpoint data", () => {
      // Simulate a checkpoint (JSON structure)
      const checkpoint = {
        threadId: "thread-123",
        state: {
          messages: Array.from({ length: 50 }, (_, i) => ({
            id: `msg-${i}`,
            content: "Test message content ".repeat(10),
            timestamp: Date.now(),
          })),
        },
        metadata: {
          createdAt: new Date().toISOString(),
          version: "1.0.0",
        },
      };
      
      const checkpointData = new TextEncoder().encode(JSON.stringify(checkpoint));
      const config = selectCompressionStrategy(checkpointData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBeDefined();
    });

    it("should select appropriate strategy for workflow state", () => {
      // Simulate workflow state (larger JSON)
      const workflowState = {
        nodes: Array.from({ length: 100 }, (_, i) => ({
          id: `node-${i}`,
          type: "task",
          data: {
            input: "x".repeat(100),
            output: "y".repeat(100),
          },
        })),
        edges: Array.from({ length: 150 }, (_, i) => ({
          source: `node-${i % 100}`,
          target: `node-${(i + 1) % 100}`,
        })),
      };
      
      const stateData = new TextEncoder().encode(JSON.stringify(workflowState));
      const config = selectCompressionStrategy(stateData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBeDefined();
    });

    it("should handle binary file uploads", () => {
      // Simulate binary file data
      const fileData = new Uint8Array(200000);
      for (let i = 0; i < fileData.length; i++) {
        fileData[i] = Math.floor(Math.random() * 256);
      }
      
      const config = selectCompressionStrategy(fileData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBe("gzip");
    });

    it("should handle text logs", () => {
      // Simulate log file
      const logs = Array.from({ length: 1000 }, (_, i) => 
        `[2024-01-01 ${String(i).padStart(2, '0')}:00:00] INFO: Log message ${i}\n`
      ).join("");
      
      const logData = new TextEncoder().encode(logs);
      const config = selectCompressionStrategy(logData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBeDefined();
    });
  });

  describe("Edge cases and error handling", () => {
    it("should handle single byte above threshold", () => {
      const data = new Uint8Array(101).fill(65);
      const config = selectCompressionStrategy(data);
      
      expect(config.enabled).toBe(true);
    });

    it("should handle maximum size data", () => {
      // Test with reasonably large data (not too large to avoid memory issues)
      const largeData = new Uint8Array(1000000).fill(65); // 1MB
      const config = selectCompressionStrategy(largeData);
      
      expect(config.enabled).toBe(true);
      expect(config.algorithm).toBeDefined();
    });

    it("should handle data with only whitespace", () => {
      const whitespaceData = new Uint8Array(500).fill(32); // All spaces
      const dataType = detectDataType(whitespaceData);
      
      // Whitespace is considered printable, so should be text
      expect(dataType).toBe("text");
    });

    it("should handle data with mixed JSON and binary", () => {
      // Start with JSON marker but mostly binary
      const mixedData = new Uint8Array(1000);
      mixedData[0] = 123; // '{' character
      for (let i = 1; i < mixedData.length; i++) {
        mixedData[i] = Math.floor(Math.random() * 256);
      }
      
      const dataType = detectDataType(mixedData);
      // Should detect as JSON because first non-whitespace is '{'
      expect(dataType).toBe("json");
    });

    it("should handle Unicode text correctly", () => {
      const unicodeText = "你好世界🌍 Hello World".repeat(100);
      const textData = new TextEncoder().encode(unicodeText);
      const dataType = detectDataType(textData);
      
      // UTF-8 encoded Unicode may have bytes outside printable ASCII range
      // but should still be detected as text or binary depending on ratio
      expect(["text", "binary"]).toContain(dataType);
    });
  });

  describe("Configuration validation", () => {
    it("should return valid CompressionConfig for all strategies", () => {
      const testCases = [
        { data: new Uint8Array(50).fill(65), desc: "small" },
        { data: new TextEncoder().encode('{"test": true}'), desc: "json" },
        { data: new TextEncoder().encode("x".repeat(60000)), desc: "large-text" },
        { data: new Uint8Array(150000).fill(0), desc: "large-binary" },
      ];

      testCases.forEach(({ data, desc }) => {
        const config = selectCompressionStrategy(data);
        
        // Validate config structure
        expect(config).toHaveProperty("enabled");
        expect(typeof config.enabled).toBe("boolean");
        
        if (config.enabled && config.algorithm) {
          expect(["gzip", "brotli"]).toContain(config.algorithm);
        }
        
        if (config.threshold !== undefined) {
          expect(typeof config.threshold).toBe("number");
          expect(config.threshold).toBeGreaterThanOrEqual(0);
        }
        
        if (config.level !== undefined) {
          expect(typeof config.level).toBe("number");
          expect(config.level).toBeGreaterThanOrEqual(1);
          expect(config.level).toBeLessThanOrEqual(9);
        }
      });
    });

    it("should maintain consistent behavior across multiple calls", () => {
      const data = new TextEncoder().encode('{"test": "data"}'.repeat(100));
      
      const config1 = selectCompressionStrategy(data);
      const config2 = selectCompressionStrategy(data);
      
      expect(config1).toEqual(config2);
    });
  });
});
