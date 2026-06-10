/**
 * File Permission Checker Unit Tests
 */

import { describe, it, expect } from "vitest";
import {
  checkFilePermission,
  matchesPattern,
  getEffectivePermission,
  batchCheckFilePermissions,
  createDefaultFilePermissionSettings,
} from "../file-permission-checker.js";
import type { FilePermissionSettings } from "@wf-agent/types";

describe("matchesPattern", () => {
  it("should match exact patterns", () => {
    expect(matchesPattern("/workspace/src/index.ts", "**/index.ts")).toBe(true);
  });

  it("should match with wildcards", () => {
    expect(matchesPattern("/workspace/src/index.ts", "**/*.ts")).toBe(true);
    expect(matchesPattern("/workspace/src/index.js", "**/*.ts")).toBe(false);
  });

  it("should be case-insensitive on Windows-style paths", () => {
    expect(matchesPattern("C:/Project/SECRETS/key.txt", "**/secrets/**")).toBe(true);
    expect(matchesPattern("C:/Project/.ENV", "**/.env")).toBe(true);
  });

  it("should normalize backslashes to forward slashes", () => {
    expect(matchesPattern("C:\\Project\\src\\file.ts", "**/src/*.ts")).toBe(true);
    expect(matchesPattern("C:\\Project\\.env", "**/.env")).toBe(true);
  });

  it("should not match when no pattern matches", () => {
    expect(matchesPattern("/workspace/random.txt", "**/*.ts")).toBe(false);
  });
});

describe("checkFilePermission", () => {
  const settings: FilePermissionSettings = {
    rules: [
      { pattern: "**/.env", permission: "denied", description: "Env files" },
      { pattern: "**/package.json", permission: "read", description: "Package config" },
      { pattern: "**/src/**", permission: "write", description: "Source files" },
      { pattern: "**/config/**", permission: "full", description: "Config files" },
      { pattern: "**/public/**", permission: "none", description: "Public files" },
    ],
    defaultPermission: "write",
  };

  it("should deny access to matched denied files", () => {
    const result = checkFilePermission("/workspace/.env", "read", settings);
    expect(result.allowed).toBe(false);
    expect(result.reason).toContain(".env");
  });

  it("should allow read on read-only files", () => {
    const result = checkFilePermission("/workspace/package.json", "read", settings);
    expect(result.allowed).toBe(true);
  });

  it("should deny write on read-only files", () => {
    const result = checkFilePermission("/workspace/package.json", "write", settings);
    expect(result.allowed).toBe(false);
    expect(result.reason).toContain("read-only");
  });

  it("should allow read and write on write-level files", () => {
    const readResult = checkFilePermission("/workspace/src/index.ts", "read", settings);
    expect(readResult.allowed).toBe(true);

    const writeResult = checkFilePermission("/workspace/src/index.ts", "write", settings);
    expect(writeResult.allowed).toBe(true);
  });

  it("should deny delete on write-level files", () => {
    const result = checkFilePermission("/workspace/src/index.ts", "delete", settings);
    expect(result.allowed).toBe(false);
  });

  it("should allow all operations on full-level files", () => {
    expect(checkFilePermission("/workspace/config/app.json", "read", settings).allowed).toBe(true);
    expect(checkFilePermission("/workspace/config/app.json", "write", settings).allowed).toBe(true);
    expect(checkFilePermission("/workspace/config/app.json", "delete", settings).allowed).toBe(true);
  });

  it("should allow all operations on none-level files", () => {
    expect(checkFilePermission("/workspace/public/index.html", "read", settings).allowed).toBe(true);
    expect(checkFilePermission("/workspace/public/index.html", "write", settings).allowed).toBe(true);
    expect(checkFilePermission("/workspace/public/index.html", "delete", settings).allowed).toBe(true);
  });

  it("should use default permission when no rule matches", () => {
    const result = checkFilePermission("/workspace/unknown.txt", "read", settings);
    expect(result.allowed).toBe(true);
  });

  it("should return the matched rule", () => {
    const result = checkFilePermission("/workspace/.env", "read", settings);
    expect(result.matchedRule).toBeDefined();
    expect(result.matchedRule!.pattern).toBe("**/.env");
  });

  it("should return undefined matchedRule when no rule matches", () => {
    const result = checkFilePermission("/workspace/unknown.txt", "read", settings);
    expect(result.matchedRule).toBeUndefined();
  });

  it("should respect first-match-wins ordering", () => {
    const customSettings: FilePermissionSettings = {
      rules: [
        { pattern: "**/src/**", permission: "denied", description: "Deny all src" },
        { pattern: "**/src/partials/**", permission: "read", description: "Allow partials" },
      ],
      defaultPermission: "write",
    };
    // First rule (denied) wins over second (read) for src/partials
    const result = checkFilePermission("/workspace/src/partials/header.ts", "read", customSettings);
    expect(result.allowed).toBe(false);
  });

  it("should handle empty rules array", () => {
    const emptySettings: FilePermissionSettings = {
      rules: [],
      defaultPermission: "write",
    };
    const result = checkFilePermission("/workspace/file.txt", "read", emptySettings);
    expect(result.allowed).toBe(true);
  });

  it("should handle denied default permission", () => {
    const denyDefault: FilePermissionSettings = {
      rules: [],
      defaultPermission: "denied",
    };
    const result = checkFilePermission("/workspace/any.txt", "read", denyDefault);
    expect(result.allowed).toBe(false);
  });
});

describe("getEffectivePermission", () => {
  it("should return the matched rule permission", () => {
    const settings: FilePermissionSettings = {
      rules: [{ pattern: "**/.env", permission: "denied" }],
      defaultPermission: "write",
    };
    expect(getEffectivePermission("/workspace/.env", settings)).toBe("denied");
  });

  it("should return default permission when no rule matches", () => {
    const settings: FilePermissionSettings = {
      rules: [{ pattern: "**/.env", permission: "denied" }],
      defaultPermission: "write",
    };
    expect(getEffectivePermission("/workspace/readme.md", settings)).toBe("write");
  });

  it("should fall back to write when defaultPermission is undefined", () => {
    const settings: FilePermissionSettings = {
      rules: [],
    };
    expect(getEffectivePermission("/workspace/file.txt", settings)).toBe("write");
  });
});

describe("batchCheckFilePermissions", () => {
  it("should check multiple files and return results map", () => {
    const settings: FilePermissionSettings = {
      rules: [
        { pattern: "**/.env", permission: "denied" },
        { pattern: "**/src/**", permission: "write" },
      ],
      defaultPermission: "write",
    };

    const files = [
      { path: "/workspace/.env", operation: "read" as const },
      { path: "/workspace/src/index.ts", operation: "write" as const },
      { path: "/workspace/unknown.txt", operation: "read" as const },
    ];

    const results = batchCheckFilePermissions(files, settings);
    expect(results.size).toBe(3);
    expect(results.get("/workspace/.env")!.allowed).toBe(false);
    expect(results.get("/workspace/src/index.ts")!.allowed).toBe(true);
    expect(results.get("/workspace/unknown.txt")!.allowed).toBe(true);
  });

  it("should handle empty files array", () => {
    const settings: FilePermissionSettings = { rules: [], defaultPermission: "write" };
    const results = batchCheckFilePermissions([], settings);
    expect(results.size).toBe(0);
  });
});

describe("createDefaultFilePermissionSettings", () => {
  it("should include sensitive file patterns as denied", () => {
    const settings = createDefaultFilePermissionSettings();
    const deniedPatterns = settings.rules.filter((r) => r.permission === "denied");
    const deniedPatternStrings = deniedPatterns.map((r) => r.pattern);
    expect(deniedPatternStrings).toContain("**/.env");
    expect(deniedPatternStrings).toContain("**/.env.*");
    expect(deniedPatternStrings).toContain("**/credentials.json");
    expect(deniedPatternStrings).toContain("**/*.pem");
    expect(deniedPatternStrings).toContain("**/*.key");
  });

  it("should include read-only config patterns", () => {
    const settings = createDefaultFilePermissionSettings();
    const readPatterns = settings.rules.filter((r) => r.permission === "read");
    const readPatternStrings = readPatterns.map((r) => r.pattern);
    expect(readPatternStrings).toContain("**/package.json");
    expect(readPatternStrings).toContain("**/tsconfig.json");
    expect(readPatternStrings).toContain("**/.git/**");
  });

  it("should set defaultPermission to write", () => {
    const settings = createDefaultFilePermissionSettings();
    expect(settings.defaultPermission).toBe("write");
  });

  it("should add workspace-specific rules when workspaceDir is provided", () => {
    const settings = createDefaultFilePermissionSettings("/workspace/my-project");
    const fullPatterns = settings.rules.filter((r) => r.permission === "full");
    const fullPatternStrings = fullPatterns.map((r) => r.pattern);
    expect(fullPatternStrings).toContain("/workspace/my-project/src/**");
    expect(fullPatternStrings).toContain("/workspace/my-project/lib/**");
  });
});