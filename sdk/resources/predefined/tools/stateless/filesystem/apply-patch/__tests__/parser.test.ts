/**
 * Tests for the patch parser
 */

import { describe, it, expect } from "vitest";
import { parsePatch } from "../utils/parser.js";
import { PatchParseError, ToolErrorCode } from "@wf-agent/types";

describe("parsePatch", () => {
  describe("basic parsing", () => {
    it("should parse a simple AddFile patch", () => {
      const patch = `*** Begin Patch
*** Add File: test.txt
+Hello World
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      expect(result.hunks[0]).toEqual({
        type: "AddFile",
        path: "test.txt",
        contents: "Hello World\n",
      });
    });

    it("should parse a simple DeleteFile patch", () => {
      const patch = `*** Begin Patch
*** Delete File: old.txt
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      expect(result.hunks[0]).toEqual({
        type: "DeleteFile",
        path: "old.txt",
      });
    });

    it("should parse a simple UpdateFile patch", () => {
      const patch = `*** Begin Patch
*** Update File: test.txt
@@
-Hello
+Hello World
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      expect(result.hunks[0].type).toBe("UpdateFile");
      if (result.hunks[0].type === "UpdateFile") {
        expect(result.hunks[0].path).toBe("test.txt");
        expect(result.hunks[0].chunks).toHaveLength(1);
        expect(result.hunks[0].chunks[0].oldLines).toEqual(["Hello"]);
        expect(result.hunks[0].chunks[0].newLines).toEqual(["Hello World"]);
      }
    });
  });

  describe("heredoc support", () => {
    it("should handle heredoc-wrapped patches", () => {
      const patch = `<<EOF
*** Begin Patch
*** Add File: test.txt
+Hello World
*** End Patch
EOF`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      expect(result.hunks[0]).toEqual({
        type: "AddFile",
        path: "test.txt",
        contents: "Hello World\n",
      });
    });

    it("should handle heredoc with single quotes", () => {
      const patch = `<<'EOF'
*** Begin Patch
*** Add File: test.txt
+Hello World
*** End Patch
EOF`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
    });
  });

  describe("path validation", () => {
    it("should reject absolute paths", () => {
      const patch = `*** Begin Patch
*** Add File: /etc/passwd
+malicious
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow();
      try {
        parsePatch(patch);
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        if (error instanceof Error && "code" in error) {
          expect((error as any).code).toBe(ToolErrorCode.PATCH_ABSOLUTE_PATH_NOT_ALLOWED);
        }
      }
    });

    it("should reject path traversal", () => {
      const patch = `*** Begin Patch
*** Add File: ../../../etc/passwd
+malicious
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow();
      try {
        parsePatch(patch);
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        if (error instanceof Error && "code" in error) {
          expect((error as any).code).toBe(ToolErrorCode.PATCH_PATH_TRAVERSAL_DETECTED);
        }
      }
    });

    it("should reject invalid filename characters", () => {
      const patch = `*** Begin Patch
*** Add File: test<file>.txt
+content
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow();
      try {
        parsePatch(patch);
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        if (error instanceof Error && "code" in error) {
          expect((error as any).code).toBe(ToolErrorCode.PATCH_INVALID_FILENAME_CHARACTERS);
        }
      }
    });
  });

  describe("error handling", () => {
    it("should reject missing begin marker", () => {
      const patch = `*** Add File: test.txt
+Hello
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow(PatchParseError);
    });

    it("should reject missing end marker", () => {
      const patch = `*** Begin Patch
*** Add File: test.txt
+Hello`;

      expect(() => parsePatch(patch)).toThrow(PatchParseError);
    });

    it("should reject invalid hunk header", () => {
      const patch = `*** Begin Patch
*** Invalid: test.txt
+Hello
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow(PatchParseError);
    });

    it("should reject invalid AddFile content", () => {
      const patch = `*** Begin Patch
*** Add File: test.txt
+Hello
invalid line
*** End Patch`;

      expect(() => parsePatch(patch)).toThrow(PatchParseError);
    });
  });

  describe("complex patches", () => {
    it("should parse multiple hunks", () => {
      const patch = `*** Begin Patch
*** Add File: new.txt
+New file
*** Delete File: old.txt
*** Update File: update.txt
@@
-old
+new
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(3);
      expect(result.hunks[0].type).toBe("AddFile");
      expect(result.hunks[1].type).toBe("DeleteFile");
      expect(result.hunks[2].type).toBe("UpdateFile");
    });

    it("should parse UpdateFile with move", () => {
      const patch = `*** Begin Patch
*** Update File: old.txt
*** Move to: new.txt
@@
-old
+new
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      if (result.hunks[0].type === "UpdateFile") {
        expect(result.hunks[0].movePath).toBe("new.txt");
      }
    });

    it("should parse multiple chunks", () => {
      const patch = `*** Begin Patch
*** Update File: test.txt
@@
-old1
+new1
@@
-old2
+new2
*** End Patch`;

      const result = parsePatch(patch);
      expect(result.hunks).toHaveLength(1);
      if (result.hunks[0].type === "UpdateFile") {
        expect(result.hunks[0].chunks).toHaveLength(2);
      }
    });
  });
});
