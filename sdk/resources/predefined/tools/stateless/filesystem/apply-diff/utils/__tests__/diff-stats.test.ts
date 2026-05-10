/**
 * Diff Statistics Utilities Unit Tests
 */

import { describe, it, expect } from "vitest";
import { computeSearchReplaceStats } from "../diff-stats.js";

describe("Diff Statistics", () => {
  describe("computeSearchReplaceStats", () => {
    it("should compute stats for simple SEARCH/REPLACE block", () => {
      const diff = `<<<<<<< SEARCH
old line 1
old line 2
=======
new line 1
>>>>>>> REPLACE`;

      const result = computeSearchReplaceStats(diff);
      expect(result).toBeDefined();
      expect(result?.additions).toBe(1);
      expect(result?.deletions).toBe(2);
    });

    it("should compute stats for multiple blocks", () => {
      const diff = `<<<<<<< SEARCH
line 1
=======
replacement 1
>>>>>>> REPLACE

<<<<<<< SEARCH
line 2
line 3
=======
replacement 2
replacement 3
replacement 4
>>>>>>> REPLACE`;

      const result = computeSearchReplaceStats(diff);
      expect(result).toBeDefined();
      // Block 1: 1 addition, Block 2: 3 additions = 4 total
      expect(result?.additions).toBe(4);
      // Block 1: 1 deletion, Block 2: 2 deletions = 3 total
      expect(result?.deletions).toBe(3);
    });

    it("should handle empty REPLACE section", () => {
      const diff = `<<<<<<< SEARCH
line to delete
=======
>>>>>>> REPLACE`;

      const result = computeSearchReplaceStats(diff);
      expect(result).toBeDefined();
      expect(result?.additions).toBe(0);
      expect(result?.deletions).toBe(1);
    });

    it("should ignore empty lines in counting", () => {
      const diff = `<<<<<<< SEARCH
line 1

line 2
=======
new line 1

new line 2
>>>>>>> REPLACE`;

      const result = computeSearchReplaceStats(diff);
      expect(result).toBeDefined();
      expect(result?.additions).toBe(2);
      expect(result?.deletions).toBe(2);
    });

    it("should return undefined for invalid diff", () => {
      const result = computeSearchReplaceStats("invalid diff");
      expect(result).toBeUndefined();
    });

    it("should return undefined for empty string", () => {
      const result = computeSearchReplaceStats("");
      expect(result).toBeUndefined();
    });

    it("should return undefined for null input", () => {
      const result = computeSearchReplaceStats(null as any);
      expect(result).toBeUndefined();
    });

    it("should return undefined for undefined input", () => {
      const result = computeSearchReplaceStats(undefined as any);
      expect(result).toBeUndefined();
    });

    it("should handle whitespace-only lines", () => {
      const diff = `<<<<<<< SEARCH
line 1
   
line 2
=======
new line
>>>>>>> REPLACE`;

      const result = computeSearchReplaceStats(diff);
      expect(result).toBeDefined();
      expect(result?.additions).toBe(1);
      expect(result?.deletions).toBe(2);
    });
  });
});
