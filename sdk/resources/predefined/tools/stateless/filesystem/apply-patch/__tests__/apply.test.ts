/**
 * Tests for the patch application logic
 */

import { describe, it, expect } from "vitest";
import { applyChunksToContent } from "../utils/apply.js";
import { PatchApplyError, ToolErrorCode } from "@wf-agent/types";

describe("applyChunksToContent", () => {
  describe("basic operations", () => {
    it("should replace a single line", () => {
      const content = "Hello\nWorld\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["Hello"],
          newLines: ["Hi"],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("Hi\nWorld\n");
    });

    it("should add a line", () => {
      const content = "Hello\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: [],
          newLines: ["World"],
          isEndOfFile: true,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("Hello\nWorld\n");
    });

    it("should delete a line", () => {
      const content = "Hello\nWorld\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["World"],
          newLines: [],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("Hello\n");
    });
  });

  describe("context matching", () => {
    it("should use change context to narrow search", () => {
      const content = `function foo() {
  console.log("foo");
}

function bar() {
  console.log("bar");
}
`;
      const chunks = [
        {
          changeContext: "function bar() {",
          oldLines: ['console.log("bar");'],
          newLines: ['console.log("BAR");'],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toContain('console.log("BAR");');
      expect(result).toContain('console.log("foo");');
    });

    it("should throw error if context not found", () => {
      const content = "Hello\nWorld\n";
      const chunks = [
        {
          changeContext: "NonExistent",
          oldLines: ["Hello"],
          newLines: ["Hi"],
          isEndOfFile: false,
        },
      ];
      expect(() => applyChunksToContent(content, "test.txt", chunks)).toThrow(PatchApplyError);
    });
  });

  describe("multiple chunks", () => {
    it("should apply multiple chunks in order", () => {
      const content = "a\nb\nc\nd\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["a"],
          newLines: ["A"],
          isEndOfFile: false,
        },
        {
          changeContext: null,
          oldLines: ["c"],
          newLines: ["C"],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("A\nb\nC\nd\n");
    });

    it("should handle overlapping chunks correctly", () => {
      const content = "a\nb\nc\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["a", "b"],
          newLines: ["x"],
          isEndOfFile: false,
        },
        {
          changeContext: null,
          oldLines: ["c"],
          newLines: ["y"],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("x\ny\n");
    });
  });

  describe("eof handling", () => {
    it("should match at end of file when isEndOfFile is true", () => {
      const content = "a\nb\na\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["a"],
          newLines: ["z"],
          isEndOfFile: true,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("a\nb\nz\n");
    });
  });

  describe("error handling", () => {
    it("should throw error if old lines not found", () => {
      const content = "Hello\nWorld\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["NonExistent"],
          newLines: ["Hi"],
          isEndOfFile: false,
        },
      ];
      expect(() => applyChunksToContent(content, "test.txt", chunks)).toThrow(PatchApplyError);
      try {
        applyChunksToContent(content, "test.txt", chunks);
      } catch (error) {
        expect(error).toBeInstanceOf(PatchApplyError);
        if (error instanceof PatchApplyError) {
          expect(error.code).toBe(ToolErrorCode.PATCH_OLD_LINES_NOT_FOUND);
        }
      }
    });
  });

  describe("edge cases", () => {
    it("should handle empty file", () => {
      const content = "";
      const chunks = [
        {
          changeContext: null,
          oldLines: [],
          newLines: ["Hello"],
          isEndOfFile: true,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("Hello\n");
    });

    it("should preserve trailing newline", () => {
      const content = "Hello\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["Hello"],
          newLines: ["World"],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("World\n");
    });

    it("should handle context lines", () => {
      const content = "a\nb\nc\n";
      const chunks = [
        {
          changeContext: null,
          oldLines: ["a", "b", "c"],
          newLines: ["x", "y", "z"],
          isEndOfFile: false,
        },
      ];
      const result = applyChunksToContent(content, "test.txt", chunks);
      expect(result).toBe("x\ny\nz\n");
    });
  });
});
