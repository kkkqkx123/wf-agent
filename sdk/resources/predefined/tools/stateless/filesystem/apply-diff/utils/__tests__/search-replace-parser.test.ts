/**
 * Search/Replace Parser Unit Tests
 */

import { describe, it, expect } from "vitest";
import { parseSearchReplaceBlocks, validateMarkerSequencing } from "../search-replace-parser.js";

describe("Search/Replace Parser", () => {
  describe("parseSearchReplaceBlocks", () => {
    it("should parse simple SEARCH/REPLACE block", () => {
      const diff = `<<<<<<< SEARCH
old content
=======
new content
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].searchContent).toBe("old content");
      expect(result.blocks[0].replaceContent).toBe("new content");
    });

    it("should parse block with line numbers", () => {
      const diff = `<<<<<<< SEARCH
:start_line:10
:end_line:15
-------
old content
=======
new content
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].startLine).toBe(10);
      expect(result.blocks[0].endLine).toBe(15);
      expect(result.blocks[0].searchContent).toBe("old content");
      expect(result.blocks[0].replaceContent).toBe("new content");
    });

    it("should parse multiple blocks", () => {
      const diff = `<<<<<<< SEARCH
first old
=======
first new
>>>>>>> REPLACE

<<<<<<< SEARCH
second old
=======
second new
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(2);
      expect(result.blocks[0].searchContent).toBe("first old");
      expect(result.blocks[0].replaceContent).toBe("first new");
      expect(result.blocks[1].searchContent).toBe("second old");
      expect(result.blocks[1].replaceContent).toBe("second new");
    });

    it("should handle multiline content", () => {
      const diff = `<<<<<<< SEARCH
line 1
line 2
line 3
=======
replacement 1
replacement 2
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].searchContent).toContain("line 1");
      expect(result.blocks[0].searchContent).toContain("line 2");
      expect(result.blocks[0].replaceContent).toContain("replacement 1");
    });

    it("should unescape markers in content", () => {
      const diff = `<<<<<<< SEARCH
\\<<<<<<< escaped search
normal line
=======
\\>>>>>>> escaped replace
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks[0].searchContent).toContain("<<<<<<< escaped search");
      expect(result.blocks[0].replaceContent).toContain(">>>>>>> escaped replace");
    });

    it("should return error for invalid format", () => {
      const diff = "not a valid diff format";
      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeDefined();
      expect(result.blocks).toHaveLength(0);
    });

    it("should return error for missing REPLACE marker", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeDefined();
      expect(result.blocks).toHaveLength(0);
    });

    it("should return error for missing SEARCH marker", () => {
      const diff = `content
=======
replacement
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeDefined();
      expect(result.blocks).toHaveLength(0);
    });

    it("should handle empty search content", () => {
      const diff = `<<<<<<< SEARCH
=======
new content
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].searchContent).toBe("");
      expect(result.blocks[0].replaceContent).toBe("new content");
    });

    it("should handle empty replace content", () => {
      const diff = `<<<<<<< SEARCH
old content
=======
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].searchContent).toBe("old content");
      expect(result.blocks[0].replaceContent).toBe("");
    });

    it("should parse block without ------- separator", () => {
      const diff = `<<<<<<< SEARCH
:start_line:5
old content
=======
new content
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0].startLine).toBe(5);
    });

    it("should handle optional start_line only", () => {
      const diff = `<<<<<<< SEARCH
:start_line:10
-------
content
=======
replacement
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks[0].startLine).toBe(10);
      expect(result.blocks[0].endLine).toBeUndefined();
    });

    it("should handle optional end_line only", () => {
      const diff = `<<<<<<< SEARCH
:end_line:20
-------
content
=======
replacement
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks[0].startLine).toBeUndefined();
      expect(result.blocks[0].endLine).toBe(20);
    });

    it("should preserve whitespace in content", () => {
      const diff = `<<<<<<< SEARCH
  indented line
    more indented
=======
  new indented
>>>>>>> REPLACE`;

      const result = parseSearchReplaceBlocks(diff);
      expect(result.error).toBeUndefined();
      expect(result.blocks[0].searchContent).toContain("  indented line");
    });
  });

  describe("validateMarkerSequencing", () => {
    it("should validate correct sequence", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it("should validate multiple correct blocks", () => {
      const diff = `<<<<<<< SEARCH
content1
=======
replacement1
>>>>>>> REPLACE

<<<<<<< SEARCH
content2
=======
replacement2
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

    it("should detect missing separator", () => {
      const diff = `<<<<<<< SEARCH
content
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(false);
      expect(result.error).toContain("Invalid marker sequence");
    });

    it("should detect incomplete block", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(false);
      expect(result.error).toContain("Incomplete SEARCH/REPLACE block");
    });

    it("should detect double SEARCH markers", () => {
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

    it("should detect double separators", () => {
      const diff = `<<<<<<< SEARCH
content
=======
=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(false);
      expect(result.error).toContain("Invalid marker sequence");
    });

    it("should detect REPLACE before separator", () => {
      const diff = `<<<<<<< SEARCH
content
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(false);
      expect(result.error).toContain("Invalid marker sequence");
    });

    it("should handle text before first marker", () => {
      const diff = `Some introductory text
<<<<<<< SEARCH
content
=======
replacement
>>>>>>> REPLACE`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(true);
    });

    it("should handle text after last marker", () => {
      const diff = `<<<<<<< SEARCH
content
=======
replacement
>>>>>>> REPLACE
Some trailing text`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(true);
    });

    it("should handle empty string", () => {
      const result = validateMarkerSequencing("");
      expect(result.success).toBe(true);
    });

    it("should handle text without markers", () => {
      const result = validateMarkerSequencing("just plain text");
      expect(result.success).toBe(true);
    });

    it("should detect REPLACE marker at start", () => {
      const diff = `>>>>>>> REPLACE
content`;

      const result = validateMarkerSequencing(diff);
      expect(result.success).toBe(false);
      expect(result.error).toContain("Expected SEARCH marker first");
    });
  });
});
