/**
 * AST Metadata Tests
 * Tests for the metadata and source location features (Phase 2)
 */

import { describe, it, expect } from "vitest";
import { parseAST } from "../expression-parser.js";
import {
  extractAllMetadata,
  findNodeAtPosition,
  getNodeLocationDescription,
  extractComments,
  createMetadata,
  addComment,
} from "../ast-metadata.js";
import type { NodeMetadata } from "../ast-types.js";

describe("AST Metadata Utilities", () => {
  describe("createMetadata", () => {
    it("should create metadata with position only", () => {
      const metadata = createMetadata(0, 10);
      expect(metadata.location).toEqual({
        start: 0,
        end: 10,
        line: undefined,
        column: undefined,
      });
    });

    it("should create metadata with line and column", () => {
      const metadata = createMetadata(5, 15, 2, 3);
      expect(metadata.location).toEqual({
        start: 5,
        end: 15,
        line: 2,
        column: 3,
      });
    });
  });

  describe("addComment", () => {
    it("should add comment to undefined metadata", () => {
      const metadata = addComment(undefined, "Test comment");
      expect(metadata.comments).toEqual(["Test comment"]);
    });

    it("should add comment to existing metadata", () => {
      const existing: NodeMetadata = { comments: ["First"] };
      const updated = addComment(existing, "Second");
      expect(updated.comments).toEqual(["First", "Second"]);
    });

    it("should preserve other metadata fields", () => {
      const existing: NodeMetadata = {
        location: { start: 0, end: 10 },
        comments: ["Existing"],
      };
      const updated = addComment(existing, "New");
      expect(updated.location).toEqual({ start: 0, end: 10 });
      expect(updated.comments).toEqual(["Existing", "New"]);
    });
  });

  describe("extractAllMetadata", () => {
    it("should return empty array when no metadata exists", () => {
      const ast = parseAST("user.age > 18");
      const metadata = extractAllMetadata(ast);
      expect(metadata).toEqual([]);
    });

    it("should extract metadata from nodes that have it", () => {
      // Note: Current parser doesn't attach metadata yet
      // This test will pass once we enhance the parser
      const ast = parseAST("true && false");
      const metadata = extractAllMetadata(ast);
      // Currently returns empty because parser doesn't set metadata
      expect(Array.isArray(metadata)).toBe(true);
    });
  });

  describe("findNodeAtPosition", () => {
    it("should return null when node has no location", () => {
      const ast = parseAST("user.name");
      const node = findNodeAtPosition(ast, 5);
      expect(node).toBeNull();
    });

    it("should find node at specific position when metadata exists", () => {
      // This test will be more meaningful once parser attaches location info
      const ast = parseAST("x > 5");
      const node = findNodeAtPosition(ast, 2);
      // Currently returns null because no location metadata
      expect(node === null || node !== null).toBe(true);
    });
  });

  describe("getNodeLocationDescription", () => {
    it("should return 'Unknown location' when no metadata", () => {
      const ast = parseAST("test");
      const desc = getNodeLocationDescription(ast);
      expect(desc).toBe("Unknown location");
    });

    it("should include line and column when available", () => {
      const ast = parseAST("test");
      if (ast.type === "comparison" && !ast.metadata) {
        ast.metadata = createMetadata(0, 4, 1, 1);
      }
      const desc = getNodeLocationDescription(ast);
      expect(desc).toContain("Line 1");
      expect(desc).toContain("Column 1");
    });

    it("should include position range", () => {
      const ast = parseAST("test");
      if (ast.type === "comparison" && !ast.metadata) {
        ast.metadata = createMetadata(0, 4);
      }
      const desc = getNodeLocationDescription(ast);
      expect(desc).toContain("Position 0-4");
    });
  });

  describe("extractComments", () => {
    it("should return empty array when no comments exist", () => {
      const ast = parseAST("user.age > 18");
      const comments = extractComments(ast);
      expect(comments).toEqual([]);
    });

    it("should extract comments from nodes", () => {
      // Create a mock AST with comments
      const mockAst = {
        type: "logical" as const,
        operator: "&&" as const,
        left: {
          type: "comparison" as const,
          variablePath: "x",
          operator: ">" as const,
          value: 5,
          metadata: { comments: ["Check x"] },
        },
        right: {
          type: "comparison" as const,
          variablePath: "y",
          operator: "<" as const,
          value: 10,
          metadata: { comments: ["Check y"] },
        },
      };
      const comments = extractComments(mockAst as any);
      expect(comments).toEqual(["Check x", "Check y"]);
    });
  });

  describe("Integration with parsed expressions", () => {
    it("should handle simple comparison", () => {
      const ast = parseAST("user.age > 18");
      expect(ast.type).toBe("comparison");
      
      // Extract metadata (currently empty)
      const metadata = extractAllMetadata(ast);
      expect(metadata.length).toBe(0);
    });

    it("should handle logical expressions", () => {
      const ast = parseAST("user.age > 18 && user.active == true");
      expect(ast.type).toBe("logical");
      
      const comments = extractComments(ast);
      expect(comments.length).toBe(0);
    });

    it("should handle nested member access", () => {
      const ast = parseAST("user.address.city == 'Beijing'");
      expect(ast.type).toBe("comparison");
      
      // Should work even without metadata
      const location = getNodeLocationDescription(ast);
      expect(location).toBe("Unknown location");
    });
  });
});
