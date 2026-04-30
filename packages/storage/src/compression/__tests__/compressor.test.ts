/**
 * Compression Module Tests
 * Comprehensive test suite for compression utilities
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  compressBlob,
  decompressBlob,
  compressBlobSync,
  decompressBlobSync,
  type CompressionConfig,
  DEFAULT_COMPRESSION_CONFIG,
} from "../compressor.js";

describe("Compression Module", () => {
  describe("compressBlob (async)", () => {
    it("should not compress when disabled", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const config: CompressionConfig = { enabled: false };
      const result = await compressBlob(data, config);

      expect(result.compressed).toEqual(data);
      expect(result.algorithm).toBeNull();
      expect(result.ratio).toBe(1);
    });

    it("should not compress data below threshold", async () => {
      const data = new Uint8Array(100).fill(65); // 100 bytes
      const config: CompressionConfig = {
        enabled: true,
        threshold: 1024,
      };
      const result = await compressBlob(data, config);

      expect(result.compressed).toEqual(data);
      expect(result.algorithm).toBeNull();
    });

    it("should compress large data with gzip by default", async () => {
      const largeData = new Uint8Array(2000).fill(65); // 2KB of 'A's - highly compressible
      const result = await compressBlob(largeData);

      expect(result.algorithm).toBe("gzip");
      expect(result.ratio).toBeLessThan(1);
      expect(result.originalSize).toBe(2000);
    });

    it("should compress with brotli algorithm when configured", async () => {
      const largeData = new Uint8Array(2000).fill(65);
      const config: CompressionConfig = {
        enabled: true,
        algorithm: "brotli",
        threshold: 100,
      };
      const result = await compressBlob(largeData, config);

      expect(result.algorithm).toBe("brotli");
      expect(result.ratio).toBeLessThan(1);
    });

    it("should return original data if compression doesn't reduce size", async () => {
      // Random data is hard to compress
      const randomData = new Uint8Array(2000);
      for (let i = 0; i < randomData.length; i++) {
        randomData[i] = Math.floor(Math.random() * 256);
      }

      const result = await compressBlob(randomData, {
        enabled: true,
        threshold: 100,
      });

      // Should either not compress or use original if compression didn't help
      if (result.algorithm !== null) {
        expect(result.ratio).toBeGreaterThanOrEqual(1);
      }
    });

    it("should handle empty data", async () => {
      const emptyData = new Uint8Array(0);
      const result = await compressBlob(emptyData);

      expect(result.compressed.length).toBe(0);
      expect(result.algorithm).toBeNull();
    });

    it("should respect custom threshold", async () => {
      const data = new Uint8Array(500).fill(65);
      const config: CompressionConfig = {
        enabled: true,
        threshold: 100, // Compress anything above 100 bytes
      };
      const result = await compressBlob(data, config);

      expect(result.algorithm).toBe("gzip");
    });

    it("should handle deprecated minSize option", async () => {
      const data = new Uint8Array(500).fill(65);
      const config: CompressionConfig = {
        enabled: true,
        minSize: 100, // Using deprecated option
      };
      const result = await compressBlob(data, config);

      expect(result.algorithm).toBe("gzip");
    });
  });

  describe("decompressBlob (async)", () => {
    it("should return data unchanged if not compressed", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const result = await decompressBlob(data, null);

      expect(result).toEqual(data);
    });

    it("should decompress gzip data", async () => {
      const original = new Uint8Array(2000).fill(65);
      const compressed = await compressBlob(original, {
        enabled: true,
        algorithm: "gzip",
        threshold: 100,
      });

      expect(compressed.algorithm).toBe("gzip");

      const decompressed = await decompressBlob(
        compressed.compressed,
        compressed.algorithm,
      );

      expect(decompressed).toEqual(original);
    });

    it("should decompress brotli data", async () => {
      const original = new Uint8Array(2000).fill(65);
      const compressed = await compressBlob(original, {
        enabled: true,
        algorithm: "brotli",
        threshold: 100,
      });

      expect(compressed.algorithm).toBe("brotli");

      const decompressed = await decompressBlob(
        compressed.compressed,
        compressed.algorithm,
      );

      expect(decompressed).toEqual(original);
    });

    it("should throw error for unsupported algorithm", async () => {
      const data = new Uint8Array([1, 2, 3]);
      await expect(decompressBlob(data, "invalid")).rejects.toThrow(
        "Unsupported compression algorithm: invalid",
      );
    });

    it("should throw error for corrupted data", async () => {
      const corruptedData = new Uint8Array([255, 255, 255, 255]);
      await expect(decompressBlob(corruptedData, "gzip")).rejects.toThrow(
        "Decompression failed",
      );
    });
  });

  describe("compressBlobSync", () => {
    it("should compress synchronously with gzip", () => {
      const largeData = new Uint8Array(2000).fill(65);
      const result = compressBlobSync(largeData, {
        enabled: true,
        threshold: 100,
      });

      expect(result.algorithm).toBe("gzip");
      expect(result.ratio).toBeLessThan(1);
    });

    it("should compress synchronously with brotli", () => {
      const largeData = new Uint8Array(2000).fill(65);
      const result = compressBlobSync(largeData, {
        enabled: true,
        algorithm: "brotli",
        threshold: 100,
      });

      expect(result.algorithm).toBe("brotli");
      expect(result.ratio).toBeLessThan(1);
    });

    it("should not compress small data", () => {
      const smallData = new Uint8Array([1, 2, 3, 4, 5]);
      const result = compressBlobSync(smallData);

      expect(result.algorithm).toBeNull();
      expect(result.compressed).toEqual(smallData);
    });
  });

  describe("decompressBlobSync", () => {
    it("should decompress gzip data synchronously", () => {
      const original = new Uint8Array(2000).fill(65);
      const compressed = compressBlobSync(original, {
        enabled: true,
        algorithm: "gzip",
        threshold: 100,
      });

      const decompressed = decompressBlobSync(
        compressed.compressed,
        compressed.algorithm,
      );

      expect(decompressed).toEqual(original);
    });

    it("should decompress brotli data synchronously", () => {
      const original = new Uint8Array(2000).fill(65);
      const compressed = compressBlobSync(original, {
        enabled: true,
        algorithm: "brotli",
        threshold: 100,
      });

      const decompressed = decompressBlobSync(
        compressed.compressed,
        compressed.algorithm,
      );

      expect(decompressed).toEqual(original);
    });

    it("should return data unchanged if not compressed", () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const result = decompressBlobSync(data, null);

      expect(result).toEqual(data);
    });

    it("should throw error for corrupted data", () => {
      const corruptedData = new Uint8Array([255, 255, 255, 255]);
      expect(() => decompressBlobSync(corruptedData, "gzip")).toThrow(
        "Sync decompression failed",
      );
    });
  });

  describe("round-trip compression/decompression", () => {
    it("should preserve data through async compress/decompress cycle", async () => {
      const testData = new Uint8Array(5000);
      for (let i = 0; i < testData.length; i++) {
        testData[i] = i % 256;
      }

      const compressed = await compressBlob(testData, {
        enabled: true,
        threshold: 100,
      });

      if (compressed.algorithm) {
        const decompressed = await decompressBlob(
          compressed.compressed,
          compressed.algorithm,
        );
        expect(decompressed).toEqual(testData);
      }
    });

    it("should preserve data through sync compress/decompress cycle", () => {
      const testData = new Uint8Array(5000);
      for (let i = 0; i < testData.length; i++) {
        testData[i] = i % 256;
      }

      const compressed = compressBlobSync(testData, {
        enabled: true,
        threshold: 100,
      });

      if (compressed.algorithm) {
        const decompressed = decompressBlobSync(
          compressed.compressed,
          compressed.algorithm,
        );
        expect(decompressed).toEqual(testData);
      }
    });

    it("should handle text data correctly", async () => {
      const text = "Hello, World! ".repeat(1000);
      const encoder = new TextEncoder();
      const original = encoder.encode(text);

      const compressed = await compressBlob(original, {
        enabled: true,
        threshold: 100,
      });

      if (compressed.algorithm) {
        const decompressed = await decompressBlob(
          compressed.compressed,
          compressed.algorithm,
        );
        const decoder = new TextDecoder();
        const decodedText = decoder.decode(decompressed);
        expect(decodedText).toBe(text);
      }
    });

    it("should handle JSON data correctly", async () => {
      const jsonData = {
        name: "test",
        values: Array.from({ length: 100 }, (_, i) => i),
        nested: {
          data: "value".repeat(100),
        },
      };
      const original = new TextEncoder().encode(JSON.stringify(jsonData));

      const compressed = await compressBlob(original, {
        enabled: true,
        threshold: 100,
      });

      if (compressed.algorithm) {
        const decompressed = await decompressBlob(
          compressed.compressed,
          compressed.algorithm,
        );
        const decoded = JSON.parse(new TextDecoder().decode(decompressed));
        expect(decoded).toEqual(jsonData);
      }
    });
  });

  describe("compression ratio and performance", () => {
    it("should achieve good compression ratio for repetitive data", async () => {
      const repetitiveData = new Uint8Array(10000).fill(42);
      const result = await compressBlob(repetitiveData, {
        enabled: true,
        threshold: 100,
      });

      expect(result.ratio).toBeLessThan(0.1); // Should compress to less than 10%
    });

    it("should report correct original size", async () => {
      const data = new Uint8Array(3000).fill(65);
      const result = await compressBlob(data);

      expect(result.originalSize).toBe(3000);
    });

    it("should calculate correct compression ratio", async () => {
      const data = new Uint8Array(2000).fill(65);
      const result = await compressBlob(data);

      if (result.algorithm) {
        const expectedRatio = result.compressed.length / result.originalSize;
        expect(result.ratio).toBeCloseTo(expectedRatio, 5);
      }
    });
  });

  describe("edge cases", () => {
    it("should handle single byte data", async () => {
      const data = new Uint8Array([42]);
      const result = await compressBlob(data);

      expect(result.algorithm).toBeNull(); // Too small to compress
    });

    it("should handle very large data", async () => {
      const largeData = new Uint8Array(100000).fill(65); // 100KB
      const result = await compressBlob(largeData);

      expect(result.algorithm).toBe("gzip");
      expect(result.ratio).toBeLessThan(0.1);
    });

    it("should handle all zeros", async () => {
      const zeros = new Uint8Array(1000).fill(0);
      const result = await compressBlob(zeros, {
        enabled: true,
        threshold: 100,
      });

      expect(result.algorithm).toBe("gzip");
      expect(result.ratio).toBeLessThan(0.1);
    });

    it("should handle all 255s", async () => {
      const maxValues = new Uint8Array(1000).fill(255);
      const result = await compressBlob(maxValues, {
        enabled: true,
        threshold: 100,
      });

      expect(result.algorithm).toBe("gzip");
      expect(result.ratio).toBeLessThan(0.1);
    });
  });

  describe("DEFAULT_COMPRESSION_CONFIG", () => {
    it("should have correct default values", () => {
      expect(DEFAULT_COMPRESSION_CONFIG.enabled).toBe(true);
      expect(DEFAULT_COMPRESSION_CONFIG.algorithm).toBe("gzip");
      expect(DEFAULT_COMPRESSION_CONFIG.threshold).toBe(1024);
    });
  });
});
