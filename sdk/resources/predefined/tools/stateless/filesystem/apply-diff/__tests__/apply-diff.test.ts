/**
 * Tests for apply_diff tool
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdir, writeFile, readFile, rm } from "fs/promises";
import { join } from "path";
import { createApplyDiffHandler } from "../handler.js";
import { parseSearchReplaceBlocks, validateMarkerSequencing } from "../utils/parser.js";
import { applyBlock } from "../utils/apply.js";
import type { SearchReplaceBlock } from "../utils/types.js";

const testDir = join(process.cwd(), "test-temp-apply-diff");

describe("apply_diff Tool", () => {
  beforeEach(async () => {
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(testDir, { recursive: true, force: true });
  });

  describe("Parser", () => {
    it("should parse a single SEARCH/REPLACE block", () => {
      const diff = `<<<<<<< SEARCH
const a = 1;
=======
const a = 2;
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0]!.searchContent).toBe("const a = 1;");
      expect(result.blocks[0]!.replaceContent).toBe("const a = 2;");
    });

    it("should parse multiple SEARCH/REPLACE blocks", () => {
      const diff = `<<<<<<< SEARCH
const a = 1;
=======
const a = 2;
>>>>>>> REPLACE
<<<<<<< SEARCH
const b = 3;
=======
const b = 4;
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(2);
      expect(result.blocks[0]!.searchContent).toBe("const a = 1;");
      expect(result.blocks[1]!.searchContent).toBe("const b = 3;");
    });

    it("should parse start_line hint", () => {
      const diff = `<<<<<<< SEARCH
:start_line:10
const x = 5;
=======
const x = 10;
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0]!.startLine).toBe(10);
      expect(result.blocks[0]!.searchContent).toBe("const x = 5;");
    });

    it("should return error for invalid format", () => {
      const diff = "This is not a valid diff";
      
      const result = parseSearchReplaceBlocks(diff);
      
      expect(result.error).toBeDefined();
      expect(result.blocks).toHaveLength(0);
    });

    it("should handle escaped markers in content", () => {
      const diff = `<<<<<<< SEARCH
\\<<<<<<< SEARCH
This is escaped
=======
Replaced
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0]!.searchContent).toContain("<<<<<<< SEARCH");
    });
  });

  describe("Marker Validation", () => {
    it("should validate correct marker sequence", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      
      expect(result.success).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it("should detect missing SEARCH marker", () => {
      const diff = `=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain("Expected SEARCH marker first");
    });

    it("should detect incomplete block", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement`;

      const result = validateMarkerSequencing(diff);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain("Missing closing marker");
    });

    it("should detect invalid marker sequence", () => {
      const diff = `<<<<<<< SEARCH
content
<<<<<<< SEARCH
more content
=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain("Invalid marker sequence");
    });
  });

  describe("Apply Block", () => {
    it("should apply exact match", () => {
      const lines = ["line 1", "line 2", "line 3"];
      const block: SearchReplaceBlock = {
        searchContent: "line 2",
        replaceContent: "modified line 2",
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines).toEqual(["line 1", "modified line 2", "line 3"]);
        expect(result.delta).toBe(0);
      }
    });

    it("should apply replacement with multiple lines", () => {
      const lines = ["function test() {", "  return 1;", "}"];
      const block: SearchReplaceBlock = {
        searchContent: "function test() {\n  return 1;\n}",
        replaceContent: "function test() {\n  return 2;\n}",
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines).toEqual([
          "function test() {",
          "  return 2;",
          "}",
        ]);
      }
    });

    it("should preserve indentation", () => {
      const lines = [
        "function outer() {",
        "    function inner() {",
        "        return 1;",
        "    }",
        "}",
      ];
      const block: SearchReplaceBlock = {
        searchContent: "    function inner() {\n        return 1;\n    }",
        replaceContent: "    function inner() {\n        return 2;\n    }",
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines[1]).toBe("    function inner() {");
        expect(result.lines[2]).toBe("        return 2;");
        expect(result.lines[3]).toBe("    }");
      }
    });

    it("should use startLine hint", () => {
      const lines = ["a", "b", "c", "b", "d"];
      const block: SearchReplaceBlock = {
        searchContent: "b",
        replaceContent: "B",
        startLine: 4, // Should match the second 'b'
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines).toEqual(["a", "b", "c", "B", "d"]);
      }
    });

    it("should fail when no match found", () => {
      const lines = ["line 1", "line 2"];
      const block: SearchReplaceBlock = {
        searchContent: "nonexistent",
        replaceContent: "replacement",
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("No match found");
      }
    });

    it("should fail with empty search content", () => {
      const lines = ["line 1", "line 2"];
      const block: SearchReplaceBlock = {
        searchContent: "",
        replaceContent: "replacement",
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("Empty search content");
      }
    });

    it("should handle Unicode normalization", () => {
      const lines = ['console.log("hello")'];
      const block: SearchReplaceBlock = {
        searchContent: 'console.log(\u201chello\u201d)', // Fancy quotes
        replaceContent: 'console.log("world")',
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines[0]).toBe('console.log("world")');
      }
    });

    it("should handle trailing whitespace differences", () => {
      const lines = ['const x = 1;   ']; // Trailing spaces
      const block: SearchReplaceBlock = {
        searchContent: 'const x = 1;', // No trailing spaces
        replaceContent: 'const x = 2;',
      };

      const result = applyBlock(lines, block, 0);

      expect(result.success).toBe(true);
      if (result.success && "lines" in result) {
        expect(result.lines[0]).toBe('const x = 2;');
      }
    });
  });

  describe("Handler Integration", () => {
    const handler = createApplyDiffHandler({ workspaceDir: testDir });

    it("should apply diff to a file", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "const a = 1;\nconst b = 2;\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
const a = 1;
=======
const a = 10;
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.content).toContain("Diff applied successfully");
        
        const updatedContent = await readFile(testFile, "utf-8");
        expect(updatedContent).toBe("const a = 10;\nconst b = 2;\n");
      }
    });

    it("should handle multiple blocks", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "const a = 1;\nconst b = 2;\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
const a = 1;
=======
const a = 10;
>>>>>>> REPLACE
<<<<<<< SEARCH
const b = 2;
=======
const b = 20;
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        const updatedContent = await readFile(testFile, "utf-8");
        expect(updatedContent).toBe("const a = 10;\nconst b = 20;\n");
      }
    });

    it("should fail when file not found", async () => {
      const result = await handler({
        path: "nonexistent.ts",
        diff: `<<<<<<< SEARCH
content
=======
replacement
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("File not found");
      }
    });

    it("should fail with invalid diff format", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "content\n");

      const result = await handler({
        path: "test.ts",
        diff: "invalid diff",
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("Invalid diff format");
      }
    });

    it("should report partial success", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "const a = 1;\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
const a = 1;
=======
const a = 10;
>>>>>>> REPLACE
<<<<<<< SEARCH
const b = 2;
=======
const b = 20;
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.content).toContain("Warning");
        expect(result.content).toContain("failed to apply");
      }
    });

    it("should validate required parameters", async () => {
      const result1 = await handler({});
      expect(result1.success).toBe(false);

      const result2 = await handler({ path: "test.ts" });
      expect(result2.success).toBe(false);

      const result3 = await handler({ diff: "diff" });
      expect(result3.success).toBe(false);
    });

    it("should preserve line endings", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "const a = 1;\r\nconst b = 2;\r\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
const a = 1;
=======
const a = 10;
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        const updatedContent = await readFile(testFile, "utf-8");
        expect(updatedContent).toContain("\r\n");
      }
    });
  });
});
