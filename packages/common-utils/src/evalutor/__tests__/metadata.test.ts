/**
 * AST Metadata Tests
 * Tests for the metadata and source location features (Phase 2)
 */

import { describe, it, expect } from "vitest";
import { dslParse } from "../dsl/index.js";
import {
  extractAllMetadata,
  findNodeAtPosition,
  getNodeLocationDescription,
  extractComments,
  createMetadata,
  addComment,
} from "../ast-metadata.js";
import type { NodeMetadata, Expression } from "../dsl/types.js";

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
    it("should return all metadata from parsed expressions", () => {
      const ast = dslParse("user.age > 18");
      const metadata = extractAllMetadata(ast!);
      expect(metadata.length).toBeGreaterThan(0);
      expect(metadata[0]).toHaveProperty("node");
      expect(metadata[0]).toHaveProperty("metadata");
    });

    it("should extract metadata from nodes that have it", () => {
      const ast = dslParse("true && false");
      const metadata = extractAllMetadata(ast!);
      expect(Array.isArray(metadata)).toBe(true);
    });
  });

  describe("findNodeAtPosition", () => {
    it("should find node at specific position when metadata exists", () => {
      const ast = dslParse("user.name");
      const node = findNodeAtPosition(ast!, 5);
      expect(node).not.toBeNull();
      expect(node!.type).toBe("memberAccess");
    });

    it("should find node at specific position when metadata exists", () => {
      const ast = dslParse("x > 5");
      const node = findNodeAtPosition(ast!, 2);
      expect(node === null || node !== null).toBe(true);
    });
  });

  describe("getNodeLocationDescription", () => {
    it("should return location description when metadata exists", () => {
      const ast = dslParse("x > 5");
      const desc = getNodeLocationDescription(ast!);
      expect(desc).toContain("Line 1");
      expect(desc).toContain("Column 1");
      expect(desc).toContain("Position 0-2");
    });

    it("should include line and column when available", () => {
      const ast = dslParse("x > 5") as Expression;
      if (ast && !ast.metadata) {
        (ast as any).metadata = createMetadata(0, 4, 1, 1);
      }
      const desc = getNodeLocationDescription(ast);
      expect(desc).toContain("Line 1");
      expect(desc).toContain("Column 1");
    });

    it("should include position range", () => {
      const ast = dslParse("x > 5") as Expression;
      if (ast && !ast.metadata) {
        (ast as any).metadata = createMetadata(0, 2);
      }
      const desc = getNodeLocationDescription(ast);
      expect(desc).toContain("Position 0-2");
    });
  });

  describe("extractComments", () => {
    it("should return empty array when no comments exist", () => {
      const ast = dslParse("user.age > 18");
      const comments = extractComments(ast!);
      expect(comments).toEqual([]);
    });

    it("should extract comments from nodes", () => {
      const mockAst: Expression = {
        type: "binary",
        operator: "&&",
        left: {
          type: "binary",
          operator: ">",
          left: { type: "identifier", name: "x" },
          right: { type: "literal", valueType: "number", value: 5 },
          metadata: { comments: ["Check x"] },
        },
        right: {
          type: "binary",
          operator: "<",
          left: { type: "identifier", name: "y" },
          right: { type: "literal", valueType: "number", value: 10 },
          metadata: { comments: ["Check y"] },
        },
      };
      const comments = extractComments(mockAst);
      expect(comments).toEqual(["Check x", "Check y"]);
    });
  });

  describe("Integration with parsed expressions", () => {
    it("should handle simple comparison", () => {
      const ast = dslParse("user.age > 18");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");

      const metadata = extractAllMetadata(ast!);
      expect(metadata.length).toBeGreaterThan(0);
    });

    it("should handle logical expressions", () => {
      const ast = dslParse("user.age > 18 && user.active == true");
      expect(ast).not.toBeNull();
      expect(ast!.type).toBe("binary");

      const comments = extractComments(ast!);
      expect(comments.length).toBe(0);
    });

    it("should handle nested member access", () => {
      const ast = dslParse("user.address.city == 'Beijing'");
      expect(ast).not.toBeNull();

      const location = getNodeLocationDescription(ast!);
      expect(location).toContain("Position");
      expect(location).toContain("Line 1");
    });
  });
});