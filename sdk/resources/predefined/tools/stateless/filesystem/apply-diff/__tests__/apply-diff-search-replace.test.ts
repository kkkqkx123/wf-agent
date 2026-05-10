import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createApplyDiffHandler } from "../handler.js";
import { writeFile, mkdir, rm } from "fs/promises";
import { join } from "path";
import { tmpdir } from "os";

describe("ApplyDiff with SEARCH/REPLACE format", () => {
  let testDir: string;
  let handler: ReturnType<typeof createApplyDiffHandler>;

  beforeEach(async () => {
    testDir = join(tmpdir(), `apply-diff-test-${Date.now()}`);
    await mkdir(testDir, { recursive: true });
    handler = createApplyDiffHandler({ workspaceDir: testDir });
  });

  afterEach(async () => {
    await rm(testDir, { recursive: true, force: true });
  });

  describe("Basic Operations", () => {
    it("should apply simple replacement", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "Hello World\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Hello World
=======
Hello Universe
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.content).toContain("1 block(s) processed");
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
        expect(result.content).toContain("2 block(s) processed");
      }
    });

    it("should delete content when replace is empty", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "Line 1\nLine to delete\nLine 3\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Line to delete
=======
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });
  });

  describe("Fuzzy Matching", () => {
    it("should match content with minor whitespace differences", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, 'console.log("hello")\n');

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
console.log( "hello" )
=======
console.log("world")
>>>>>>> REPLACE`,
      });

      // Should succeed with fuzzy matching (default threshold 0.9)
      expect(result.success).toBe(true);
    });

    it("should fail when content is too different", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "Original content\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Completely different text that does not match
=======
Replacement
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("No sufficiently similar match found");
      }
    });
  });

  describe("Indentation Preservation", () => {
    it("should preserve indentation when replacing", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "    function test() {\n        return true;\n    }\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
    function test() {
        return true;
    }
=======
    function test() {
        return false;
    }
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });

    it("should handle tab-based indentation", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "\tfunction test() {\n\t\treturn true;\n\t}\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
\tfunction test() {
\t\treturn true;
\t}
=======
\tfunction test() {
\t\treturn false;
\t}
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });
  });

  describe("Line Number Hints", () => {
    it("should use start_line hint for precise location", async () => {
      const testFile = join(testDir, "test.ts");
      await writeFile(testFile, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

      const result = await handler({
        path: "test.ts",
        diff: `<<<<<<< SEARCH
:start_line:3
-------
Line 3
=======
Modified Line 3
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });
  });

  describe("Error Handling", () => {
    it("should fail when file not found", async () => {
      const result = await handler({
        path: "nonexistent.txt",
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

    it("should fail with invalid marker sequence", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "content\n");

      const result = await handler({
        path: "test.txt",
        diff: `=======
invalid format
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("Invalid marker");
      }
    });

    it("should fail when search content is empty", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "content\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
=======
replacement
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain("Empty search content");
      }
    });

    it("should track consecutive mistakes", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "content\n");

      // First failure
      await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
nonexistent
=======
replacement
>>>>>>> REPLACE`,
      });

      // Second failure should include hint
      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
still nonexistent
=======
replacement
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(false);
      if (!result.success && result.error) {
        expect(result.error).toContain("Hint: Multiple failures detected");
      }
    });
  });

  describe("Edge Cases", () => {
    it("should handle Windows line endings (CRLF)", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "Line 1\r\nLine 2\r\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Line 1
=======
Modified Line 1
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });

    it("should handle special characters in content", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, 'Content with "quotes" and \'apostrophes\'\n');

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Content with "quotes" and 'apostrophes'
=======
Modified content
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });

    it("should handle escaped markers in content", async () => {
      const testFile = join(testDir, "test.txt");
      await writeFile(testFile, "Normal line\n\\<<<<<<< SEARCH\nAnother line\n");

      const result = await handler({
        path: "test.txt",
        diff: `<<<<<<< SEARCH
Normal line
\\<<<<<<< SEARCH
Another line
=======
Modified content
>>>>>>> REPLACE`,
      });

      expect(result.success).toBe(true);
    });
  });
});
