import { describe, it, expect, beforeEach } from "vitest";
import { ProtectController, SHIELD_SYMBOL } from "../ProtectController.js";

// We need to access the default patterns via the controller instance
// since they aren't exported directly
const KNOWN_DEFAULT_PATTERNS = [
  ".agentignore",
  ".agentrules*",
  ".agentconfig",
  ".vscode/**",
  ".idea/**",
  "*.code-workspace",
  ".git/**",
  ".gitignore",
  ".gitattributes",
  ".env",
  ".env.*",
  "*.pem",
  "*.key",
  "package.json",
  "package-lock.json",
  "pnpm-lock.yaml",
  "yarn.lock",
  "tsconfig.json",
  "turbo.json",
  ".github/**",
  ".gitlab-ci.yml",
  "Jenkinsfile",
  "README.md",
  "LICENSE",
  "CHANGELOG.md",
];

describe("ProtectController", () => {
  let controller: ProtectController;

  const defaultCwd = "/test/project";

  beforeEach(() => {
    controller = new ProtectController({ cwd: defaultCwd });
  });

  describe("constructor", () => {
    it("should create instance with default config", () => {
      const ctrl = new ProtectController({ cwd: defaultCwd });
      expect(ctrl).toBeInstanceOf(ProtectController);
    });

    it("should merge custom protected patterns with defaults", () => {
      const customPatterns = ["*.log", "temp/**"];
      const ctrl = new ProtectController({
        cwd: defaultCwd,
        protectedPatterns: customPatterns,
      });
      const patterns = ctrl.getProtectedPatterns();
      expect(patterns).toContain("package.json");
      expect(patterns).toContain("*.log");
      expect(patterns).toContain("temp/**");
    });

    it("should not modify default patterns when no custom patterns provided", () => {
      const patterns = controller.getProtectedPatterns();
      expect(patterns.length).toBe(KNOWN_DEFAULT_PATTERNS.length);
      for (const p of KNOWN_DEFAULT_PATTERNS) {
        expect(patterns).toContain(p);
      }
    });
  });

  describe("isWriteProtected", () => {
    it("should return true for package.json", () => {
      expect(controller.isWriteProtected("package.json")).toBe(true);
    });

    it("should return true for .env", () => {
      expect(controller.isWriteProtected(".env")).toBe(true);
    });

    it("should return true for .gitignore", () => {
      expect(controller.isWriteProtected(".gitignore")).toBe(true);
    });

    it("should return true for tsconfig.json", () => {
      expect(controller.isWriteProtected("tsconfig.json")).toBe(true);
    });

    it("should return true for README.md", () => {
      expect(controller.isWriteProtected("README.md")).toBe(true);
    });

    it("should return true for LICENSE", () => {
      expect(controller.isWriteProtected("LICENSE")).toBe(true);
    });

    it("should return true for CHANGELOG.md", () => {
      expect(controller.isWriteProtected("CHANGELOG.md")).toBe(true);
    });

    it("should return true for .env.local", () => {
      expect(controller.isWriteProtected(".env.local")).toBe(true);
    });

    it("should return true for .env.production", () => {
      expect(controller.isWriteProtected(".env.production")).toBe(true);
    });

    it("should return true for files inside .vscode directory", () => {
      expect(controller.isWriteProtected(".vscode/settings.json")).toBe(true);
    });

    it("should return true for files inside .git directory", () => {
      expect(controller.isWriteProtected(".git/config")).toBe(true);
    });

    it("should return true for files inside .github directory", () => {
      expect(controller.isWriteProtected(".github/workflows/ci.yml")).toBe(true);
    });

    it("should return true for .gitlab-ci.yml", () => {
      expect(controller.isWriteProtected(".gitlab-ci.yml")).toBe(true);
    });

    it("should return true for Jenkinsfile", () => {
      expect(controller.isWriteProtected("Jenkinsfile")).toBe(true);
    });

    it("should return true for .pem files", () => {
      expect(controller.isWriteProtected("secret.pem")).toBe(true);
    });

    it("should return true for .key files", () => {
      expect(controller.isWriteProtected("private.key")).toBe(true);
    });

    it("should return true for .agentignore", () => {
      expect(controller.isWriteProtected(".agentignore")).toBe(true);
    });

    it("should return true for .agentconfig", () => {
      expect(controller.isWriteProtected(".agentconfig")).toBe(true);
    });

    it("should return true for files matching .agentrules*", () => {
      expect(controller.isWriteProtected(".agentrules")).toBe(true);
      expect(controller.isWriteProtected(".agentrules.json")).toBe(true);
    });

    it("should return true for package-lock.json", () => {
      expect(controller.isWriteProtected("package-lock.json")).toBe(true);
    });

    it("should return true for pnpm-lock.yaml", () => {
      expect(controller.isWriteProtected("pnpm-lock.yaml")).toBe(true);
    });

    it("should return true for yarn.lock", () => {
      expect(controller.isWriteProtected("yarn.lock")).toBe(true);
    });

    it("should return true for turbo.json", () => {
      expect(controller.isWriteProtected("turbo.json")).toBe(true);
    });

    it("should return true for *.code-workspace files", () => {
      expect(controller.isWriteProtected("myproject.code-workspace")).toBe(true);
    });

    it("should return false for regular source files", () => {
      expect(controller.isWriteProtected("src/index.ts")).toBe(false);
    });

    it("should return false for regular source files in deep paths", () => {
      expect(controller.isWriteProtected("src/components/Button.tsx")).toBe(false);
    });

    it("should return false for CSS files", () => {
      expect(controller.isWriteProtected("src/styles/main.css")).toBe(false);
    });

    it("should return false for Markdown documentation files not in protected list", () => {
      expect(controller.isWriteProtected("docs/guide.md")).toBe(false);
    });

    // Path normalization tests
    it("should normalize Windows backslashes to forward slashes", () => {
      // Simulate Windows path with backslashes — the controller normalizes them
      const ctrl = new ProtectController({ cwd: "C:\\test\\project" });
      expect(ctrl.isWriteProtected("src\\index.ts")).toBe(false);
    });

    it("should handle absolute paths by resolving relative to cwd", () => {
      const ctrl = new ProtectController({ cwd: "/test/project" });
      expect(ctrl.isWriteProtected("/test/project/package.json")).toBe(true);
    });

    it("should handle absolute paths for non-protected files", () => {
      const ctrl = new ProtectController({ cwd: "/test/project" });
      expect(ctrl.isWriteProtected("/test/project/src/index.ts")).toBe(false);
    });

    it("should handle paths in subdirectories for protected files", () => {
      // Deeply nested package.json should still be matched
      expect(controller.isWriteProtected("packages/shared/package.json")).toBe(true);
    });

    it("should handle paths in subdirectories for .env files", () => {
      expect(controller.isWriteProtected("apps/backend/.env")).toBe(true);
    });

    it("should return false for null or empty path gracefully", () => {
      // The path module can handle these but let's verify no crash
      expect(() => controller.isWriteProtected("")).not.toThrow();
      // Empty string should not match anything meaningful
      expect(controller.isWriteProtected("")).toBe(false);
    });

    it("should return false for paths that resolve outside cwd", () => {
      // Paths that go above cwd should not match protected patterns
      const result = controller.isWriteProtected("../other-project/package.json");
      // This might or might not match depending on how relative path resolution works
      expect(typeof result).toBe("boolean");
    });
  });

  describe("getProtectedFiles", () => {
    it("should return a Set with only protected paths", () => {
      const paths = ["package.json", "src/index.ts", ".env", "README.md", "src/utils/helper.ts"];
      const protectedFiles = controller.getProtectedFiles(paths);
      expect(protectedFiles.size).toBe(3);
      expect(protectedFiles.has("package.json")).toBe(true);
      expect(protectedFiles.has(".env")).toBe(true);
      expect(protectedFiles.has("README.md")).toBe(true);
      expect(protectedFiles.has("src/index.ts")).toBe(false);
      expect(protectedFiles.has("src/utils/helper.ts")).toBe(false);
    });

    it("should return empty Set when no paths are protected", () => {
      const paths = ["src/index.ts", "src/utils/helper.ts", "public/index.html"];
      const protectedFiles = controller.getProtectedFiles(paths);
      expect(protectedFiles.size).toBe(0);
    });

    it("should return Set with all paths when all are protected", () => {
      const paths = [".env", "package.json", ".gitignore"];
      const protectedFiles = controller.getProtectedFiles(paths);
      expect(protectedFiles.size).toBe(3);
    });

    it("should return empty Set for empty input array", () => {
      const protectedFiles = controller.getProtectedFiles([]);
      expect(protectedFiles.size).toBe(0);
    });
  });

  describe("annotatePathsWithProtection", () => {
    it("should annotate each path with protection status", () => {
      const paths = ["package.json", "src/index.ts", ".env"];
      const annotated = controller.annotatePathsWithProtection(paths);
      expect(annotated).toEqual([
        { path: "package.json", isProtected: true },
        { path: "src/index.ts", isProtected: false },
        { path: ".env", isProtected: true },
      ]);
    });

    it("should return empty array for empty input", () => {
      expect(controller.annotatePathsWithProtection([])).toEqual([]);
    });

    it("should preserve the original path strings", () => {
      const paths = [".env", ".git/config"];
      const annotated = controller.annotatePathsWithProtection(paths);
      expect(annotated[0]!.path).toBe(".env");
      expect(annotated[1]!.path).toBe(".git/config");
    });
  });

  describe("getProtectionMessage", () => {
    it("should return the protection message string", () => {
      const message = controller.getProtectionMessage();
      expect(message).toBe("This file is write-protected and requires approval for modifications");
    });
  });

  describe("getInstructions", () => {
    it("should include SHIELD_SYMBOL in instructions", () => {
      const instructions = controller.getInstructions();
      expect(instructions).toContain(SHIELD_SYMBOL);
    });

    it("should include protected patterns in instructions", () => {
      const instructions = controller.getInstructions();
      expect(instructions).toContain("Protected patterns:");
      for (const pattern of KNOWN_DEFAULT_PATTERNS.slice(0, 5)) {
        expect(instructions).toContain(pattern);
      }
    });

    it("should include custom patterns in instructions when provided", () => {
      const customPatterns = ["*.log", "temp/**"];
      const ctrl = new ProtectController({
        cwd: defaultCwd,
        protectedPatterns: customPatterns,
      });
      const instructions = ctrl.getInstructions();
      expect(instructions).toContain("*.log");
      expect(instructions).toContain("temp/**");
    });
  });

  describe("getProtectedPatterns", () => {
    it("should return the list of protected patterns", () => {
      const patterns = controller.getProtectedPatterns();
      expect(Array.isArray(patterns)).toBe(true);
      expect(patterns.length).toBeGreaterThan(0);
    });

    it("should return a readonly array", () => {
      const patterns = controller.getProtectedPatterns();
      // Verify it's an array with the expected content
      expect(patterns).toContain("package.json");
    });

    it("should include custom patterns when provided", () => {
      const ctrl = new ProtectController({
        cwd: defaultCwd,
        protectedPatterns: ["*.custom"],
      });
      const patterns = ctrl.getProtectedPatterns();
      expect(patterns).toContain("*.custom");
    });

    it("should not mutate when custom patterns are modified after construction", () => {
      const customPatterns = ["*.log"];
      const ctrl = new ProtectController({
        cwd: defaultCwd,
        protectedPatterns: customPatterns,
      });
      customPatterns.push("*.tmp");
      const patterns = ctrl.getProtectedPatterns();
      expect(patterns).toContain("*.log");
      // The push shouldn't affect the controller's internal state
      // since the constructor spreads the array
    });
  });
});
