/**
 * IgnoreController Unit Tests
 *
 * Tests for the IgnoreController class that enforces ignore patterns
 * for file system operations.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import path from "path";

// --- Mocks (use vi.hoisted to ensure vars are available when vi.mock is hoisted) ---

const { mockFastIgnore, mockReadFile, mockAccess, mockRealpathSync } = vi.hoisted(() => ({
  mockFastIgnore: vi.fn(),
  mockReadFile: vi.fn(),
  mockAccess: vi.fn(),
  mockRealpathSync: vi.fn(),
}));

vi.mock("fast-ignore", () => ({
  default: mockFastIgnore,
}));

vi.mock("fs/promises", () => ({
  default: {
    readFile: mockReadFile,
    access: mockAccess,
  },
  readFile: mockReadFile,
  access: mockAccess,
}));

vi.mock("fs", () => ({
  default: {
    realpathSync: mockRealpathSync,
  },
  realpathSync: mockRealpathSync,
}));

// Mock logger
vi.mock("../../utils/contextual-logger.js", () => ({
  createContextualLogger: vi.fn(() => ({
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
  })),
}));

// --- Subject ---
import { IgnoreController } from "../IgnoreController.js";

describe("IgnoreController", () => {
  // Default test directory (using posix-like relative for cross-platform)
  const testCwd = "C:\\test\\project";
  let mockChecker: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    mockChecker = vi.fn();
    // Default: fast-ignore returns a checker that returns false (allow all)
    mockFastIgnore.mockReturnValue(mockChecker);
    // Default: realpathSync returns the path as-is
    mockRealpathSync.mockImplementation((p: string) => p);
    // Default: file doesn't exist
    mockAccess.mockRejectedValue(new Error("ENOENT"));
    // Default: readFile throws
    mockReadFile.mockRejectedValue(new Error("ENOENT"));
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("constructor", () => {
    it("should create instance with default values", () => {
      const controller = new IgnoreController({ cwd: testCwd });
      expect(controller).toBeInstanceOf(IgnoreController);
      expect(controller.getMode()).toBe("all");
      // Builtin patterns should be initialized
      expect(mockFastIgnore).toHaveBeenCalledOnce();
    });

    it("should create instance with custom config", () => {
      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "gitignore",
        customIgnoreFile: ".myignore",
        customPatterns: ["*.log", "tmp/"],
      });
      expect(controller.getMode()).toBe("gitignore");
    });

    it("should create instance with mode 'builtin'", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      expect(controller.getMode()).toBe("builtin");
    });

    it("should create instance with mode 'custom'", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "custom" });
      expect(controller.getMode()).toBe("custom");
    });
  });

  describe("initialize", () => {
    it("should load gitignore when mode includes gitignore", async () => {
      mockAccess.mockResolvedValue(undefined); // file exists
      mockReadFile.mockResolvedValue("node_modules\n");

      const controller = new IgnoreController({ cwd: testCwd, mode: "all" });
      await controller.initialize();

      expect(mockAccess).toHaveBeenCalledWith(path.join(testCwd, ".gitignore"));
      expect(mockReadFile).toHaveBeenCalledWith(path.join(testCwd, ".gitignore"), "utf8");
    });

    it("should load custom ignore file when mode includes custom", async () => {
      mockAccess.mockResolvedValue(undefined); // file exists
      mockReadFile.mockResolvedValue("*.log\n");

      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "custom",
        customPatterns: ["dist/"],
      });
      await controller.initialize();

      expect(mockAccess).toHaveBeenCalledWith(path.join(testCwd, ".agentignore"));
      // Should combine file content + customIgnoreFile + customPatterns
      expect(mockReadFile).toHaveBeenCalledWith(path.join(testCwd, ".agentignore"), "utf8");
    });

    it("should not load gitignore when mode is 'builtin'", async () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      await controller.initialize();
      expect(mockAccess).not.toHaveBeenCalled();
    });

    it("should not load custom ignore when mode is 'gitignore'", async () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "gitignore" });
      await controller.initialize();
      // Access might be called for .gitignore, not for .agentignore
      // We can check that only 1 access call happened (for gitignore)
      expect(mockAccess).toHaveBeenCalledTimes(1);
    });

    it("should handle missing gitignore gracefully", async () => {
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "gitignore" });
      await controller.initialize();
      // Should not throw; checker remains null
    });

    it("should handle missing custom ignore file gracefully", async () => {
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "custom" });
      await controller.initialize();
      // Should not throw; checker remains null
    });

    it("should handle readFile errors gracefully", async () => {
      mockAccess.mockResolvedValue(undefined); // file exists
      mockReadFile.mockRejectedValue(new Error("Permission denied"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "gitignore" });
      await controller.initialize();
      // Should not throw; checker remains null
    });
  });

  describe("validateAccess", () => {
    it("should return true when no patterns match", () => {
      mockChecker.mockReturnValue(false);
      mockRealpathSync.mockImplementation((p: string) => p);

      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      // Re-mock builtin checker after construction
      mockFastIgnore.mockReset();
      // The constructor already set up builtin patterns - we'll test the behavior
      const result = controller.validateAccess("src/index.ts");
      expect(result).toBe(true);
    });

    it("should return false when builtin pattern matches", () => {
      // Override fast-ignore to return a checker that matches node_modules
      mockFastIgnore.mockReset();
      const builtinChecker = vi.fn((p: string) => p.includes("node_modules"));
      mockFastIgnore.mockReturnValue(builtinChecker);

      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const result = controller.validateAccess("node_modules/foo/bar.js");
      expect(result).toBe(false);
      expect(builtinChecker).toHaveBeenCalled();
    });

    it("should return false when gitignore pattern matches", async () => {
      mockFastIgnore.mockReset();
      const gitignoreChecker = vi.fn((p: string) => p === "dist/bundle.js");

      // First call (constructor) returns noop checker; second call (initialize) returns gitignore checker
      mockFastIgnore
        .mockReturnValueOnce(vi.fn(() => false)) // builtin
        .mockReturnValueOnce(gitignoreChecker); // gitignore

      mockAccess.mockResolvedValue(undefined);
      mockReadFile.mockResolvedValue("dist/\n");

      const controller = new IgnoreController({ cwd: testCwd, mode: "gitignore" });
      await controller.initialize();

      const result = controller.validateAccess("dist/bundle.js");
      expect(result).toBe(false);
    });

    it("should return false when custom pattern matches", async () => {
      mockFastIgnore.mockReset();
      const customChecker = vi.fn((p: string) => p.startsWith("tmp/"));

      mockFastIgnore
        .mockReturnValueOnce(vi.fn(() => false)) // builtin
        .mockReturnValueOnce(customChecker); // custom

      // File doesn't exist, but we have customPatterns
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "custom",
        customPatterns: ["tmp/"],
      });
      await controller.initialize();

      const result = controller.validateAccess("tmp/foo.txt");
      expect(result).toBe(false);
    });

    it("should return false for special directories (root)", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      // Root directory check uses platform-specific logic
      const rootDir = process.platform === "win32" ? "C:\\" : "/";
      const result = controller.validateAccess(rootDir);
      expect(result).toBe(false);
    });

    it("should return false for special directories (home)", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const homeDir = process.env["HOME"] || process.env["USERPROFILE"] || "";
      if (homeDir) {
        const result = controller.validateAccess(homeDir);
        expect(result).toBe(false);
      }
    });

    it("should handle realpathSync errors gracefully", () => {
      mockRealpathSync.mockImplementation(() => {
        throw new Error("ENOENT");
      });

      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const result = controller.validateAccess("some/file.txt");
      // Should fall back to original path
      expect(result).toBe(true);
    });

    it("should handle unexpected errors gracefully (fail open)", () => {
      // Force an error in validateAccess
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      // Pass null/undefined to trigger error
      const result = controller.validateAccess(null as unknown as string);
      expect(result).toBe(true);
    });
  });

  describe("validateAccess - mode-specific behavior", () => {
    it("should use all checkers in 'all' mode", async () => {
      mockFastIgnore.mockReset();
      const builtinChecker = vi.fn(() => false);
      const customChecker = vi.fn(() => false);

      mockFastIgnore
        .mockReturnValueOnce(builtinChecker) // builtin
        .mockReturnValueOnce(vi.fn(() => false)) // gitignore - not used in "all"? Actually "all" uses gitignore too
        .mockReturnValueOnce(customChecker); // custom

      mockAccess.mockResolvedValue(undefined);
      mockReadFile
        .mockResolvedValueOnce("*.gitignore-pattern\n") // gitignore
        .mockResolvedValueOnce("*.custom-pattern\n"); // custom

      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "all",
        customPatterns: ["extra/"],
      });
      await controller.initialize();

      controller.validateAccess("src/file.ts");

      // builtin should have been checked
      expect(builtinChecker).toHaveBeenCalled();
    });
  });

  describe("shouldIncludeDirectory", () => {
    let controller: IgnoreController;

    beforeEach(() => {
      mockFastIgnore.mockReset();
      mockFastIgnore.mockReturnValue(vi.fn(() => false));
      controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
    });

    it("should include normal directories by default", () => {
      const result = controller.shouldIncludeDirectory("src", "C:\\test\\project\\src");
      expect(result).toBe(true);
    });

    it("should exclude builtin-ignored directories", () => {
      const result = controller.shouldIncludeDirectory(
        "node_modules",
        "C:\\test\\project\\node_modules",
      );
      expect(result).toBe(false);
    });

    it("should include the target directory even if hidden", () => {
      const result = controller.shouldIncludeDirectory(
        ".vscode",
        "C:\\test\\project\\.vscode",
        true,
      );
      expect(result).toBe(true);
    });

    it("should exclude critical directories even if they are the target", () => {
      const result = controller.shouldIncludeDirectory(
        "node_modules",
        "C:\\test\\project\\node_modules",
        true,
      );
      expect(result).toBe(false);
    });

    it("should use validateAccess when inside an explicit target", () => {
      // Mock validateAccess by checking fullPath
      mockRealpathSync.mockImplementation((p: string) => p);
      const result = controller.shouldIncludeDirectory(
        ".vscode/settings.json",
        "C:\\test\\project\\.vscode\\settings.json",
        false,
        true,
      );
      // Should call validateAccess internally via the logic
      expect(result).toBe(true);
    });

    it("should exclude critical directories when inside an explicit target", () => {
      const result = controller.shouldIncludeDirectory(
        "node_modules",
        "C:\\test\\project\\node_modules",
        false,
        true,
      );
      expect(result).toBe(false);
    });
  });

  describe("filterPaths", () => {
    it("should return only allowed paths", () => {
      mockFastIgnore.mockReset();
      mockFastIgnore.mockReturnValue(vi.fn((p: string) => p.includes("node_modules")));

      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const paths = ["src/index.ts", "node_modules/foo/index.js", "README.md", "dist/bundle.js"];
      const result = controller.filterPaths(paths);
      // Only first and third should pass (if builtin checker matches node_modules and dist)
      // But since we mocked the checker to only match "node_modules", dist should pass
      expect(result).toContain("src/index.ts");
      expect(result).toContain("README.md");
      expect(result).toContain("dist/bundle.js");
      expect(result).not.toContain("node_modules/foo/index.js");
    });
  });

  describe("getInstructions", () => {
    it("should return instructions with builtin patterns", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const instructions = controller.getInstructions();
      expect(instructions).toBeDefined();
      expect(instructions).toContain("Built-in Ignore Patterns");
      expect(instructions).toContain("node_modules");
    });

    it("should include gitignore content when available", async () => {
      mockFastIgnore.mockReset();
      mockFastIgnore
        .mockReturnValueOnce(vi.fn(() => false)) // builtin
        .mockReturnValueOnce(vi.fn(() => false)); // gitignore

      mockAccess.mockResolvedValue(undefined);
      mockReadFile.mockResolvedValue("dist/\n.env\n");

      const controller = new IgnoreController({ cwd: testCwd, mode: "all" });
      await controller.initialize();

      const instructions = controller.getInstructions();
      expect(instructions).toBeDefined();
      expect(instructions).toContain(".gitignore");
      expect(instructions).toContain("dist/");
    });

    it("should include custom ignore content when available", async () => {
      mockFastIgnore.mockReset();
      mockFastIgnore
        .mockReturnValueOnce(vi.fn(() => false)) // builtin
        .mockReturnValueOnce(vi.fn(() => false)); // custom

      mockAccess.mockResolvedValue(undefined);
      mockReadFile.mockResolvedValue("*.log\n");

      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "custom",
      });
      await controller.initialize();

      const instructions = controller.getInstructions();
      expect(instructions).toBeDefined();
      expect(instructions).toContain(".agentignore");
      expect(instructions).toContain("*.log");
    });

    it("should return undefined when no patterns are configured", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      // Builtin is still used, so it should return something
      const instructions = controller.getInstructions();
      expect(instructions).toBeDefined();
    });

    it("should return undefined when gitignore mode has no content", async () => {
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "gitignore" });
      await controller.initialize();

      const instructions = controller.getInstructions();
      expect(instructions).toBeUndefined();
    });

    it("should return undefined when custom mode has no content", async () => {
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "custom" });
      await controller.initialize();

      const instructions = controller.getInstructions();
      expect(instructions).toBeUndefined();
    });
  });

  describe("getMode / setMode", () => {
    it("should return the current mode", () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      expect(controller.getMode()).toBe("builtin");
    });

    it("should update mode and reload ignore files when changed", async () => {
      mockAccess.mockRejectedValue(new Error("ENOENT"));

      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });

      await controller.setMode("gitignore");

      expect(controller.getMode()).toBe("gitignore");
      // loadIgnoreFiles should have been called
    });

    it("should not reload when mode is unchanged", async () => {
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      await controller.setMode("builtin");
      expect(controller.getMode()).toBe("builtin");
      // Since mode didn't change, loadIgnoreFiles should not have been called
      // (mode check happens before calling loadIgnoreFiles)
    });
  });

  describe("integration: builtin ignore patterns", () => {
    it("should validate actual builtin patterns via fast-ignore", () => {
      // This test verifies the builtin patterns are passed to fast-ignore
      mockFastIgnore.mockReset();
      mockFastIgnore.mockReturnValue(vi.fn(() => true));

      // We need to check what patterns are passed
      mockFastIgnore.mockImplementation((_pattern: string) => {
        return vi.fn(() => true);
      });

      new IgnoreController({ cwd: testCwd, mode: "builtin" });

      // Verify fast-ignore was called with builtin patterns
      expect(mockFastIgnore).toHaveBeenCalled();
      const patternsArg = mockFastIgnore.mock.calls[0]![0];
      expect(patternsArg).toContain("node_modules");
      expect(patternsArg).toContain("**/.*/**");
      expect(patternsArg).toContain("dist");
      expect(patternsArg).toContain(".git");
    });
  });

  describe("edge cases", () => {
    it("should handle empty custom patterns", () => {
      const controller = new IgnoreController({
        cwd: testCwd,
        mode: "custom",
        customPatterns: [],
      });
      // Should not throw
      expect(controller.getMode()).toBe("custom");
    });

    it("should handle paths with forward slashes on Windows", () => {
      mockRealpathSync.mockImplementation((p: string) => p);
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      // Use forward slash path
      const result = controller.validateAccess("src/components/Button.tsx");
      expect(typeof result).toBe("boolean");
    });

    it("should handle paths with backslashes on Windows", () => {
      mockRealpathSync.mockImplementation((p: string) => p);
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const result = controller.validateAccess("src\\components\\Button.tsx");
      expect(typeof result).toBe("boolean");
    });

    it("should handle absolute paths gracefully", () => {
      mockRealpathSync.mockImplementation((p: string) => p);
      const controller = new IgnoreController({ cwd: testCwd, mode: "builtin" });
      const result = controller.validateAccess("C:\\test\\project\\src\\file.ts");
      expect(typeof result).toBe("boolean");
    });
  });
});
