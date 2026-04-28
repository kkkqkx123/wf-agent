/**
 * Ignore Controller
 *
 * Controls file access by enforcing ignore patterns.
 * Supports built-in ignore patterns, .gitignore, and custom ignore files.
 */

import * as path from "path";
import * as fs from "fs/promises";
import * as fsSync from "fs";
import fastIgnore from "fast-ignore";
import type { Options as FastIgnoreOptions } from "fast-ignore";
import { BUILTIN_IGNORE_DIRS, CRITICAL_IGNORE_DIRS } from "./constants.js";

/**
 * Ignore mode options
 */
export enum IgnoreMode {
  /** Only use built-in patterns */
  Builtin = "builtin",
  /** Only use .gitignore patterns */
  Gitignore = "gitignore",
  /** Only use custom ignore patterns */
  Custom = "custom",
  /** Use built-in and .gitignore patterns */
  BuiltinAndGitignore = "builtin-gitignore",
  /** Use built-in and custom patterns */
  BuiltinAndCustom = "builtin-custom",
  /** Use all patterns (built-in, .gitignore, and custom) */
  All = "all",
}

/**
 * Configuration options for IgnoreController
 */
export interface IgnoreControllerConfig {
  /** Working directory */
  cwd: string;
  /** Ignore mode (default: All) */
  mode?: IgnoreMode;
  /** Custom ignore file name (default: .agentignore) */
  customIgnoreFile?: string;
  /** Additional custom patterns */
  customPatterns?: string[];
}

/**
 * Check if a path is a special directory (root or home)
 */
function isSpecialDirectory(dirPath: string): boolean {
  const absolutePath = path.resolve(dirPath);

  // Check for root directory
  const root = process.platform === "win32" ? path.parse(absolutePath).root : "/";
  if (absolutePath === root) {
    return true;
  }

  // Check for home directory
  const homeDir = process.env["HOME"] || process.env["USERPROFILE"];
  if (homeDir && absolutePath === homeDir) {
    return true;
  }

  return false;
}

/**
 * Check if a directory name matches built-in ignore patterns
 */
function isBuiltinIgnored(dirName: string): boolean {
  // Check exact matches
  if (BUILTIN_IGNORE_DIRS.includes(dirName as any)) {
    return true;
  }

  // Check hidden directory pattern
  if (BUILTIN_IGNORE_DIRS.includes(".*" as any) && dirName.startsWith(".") && dirName !== ".") {
    return true;
  }

  return false;
}

/**
 * Check if a directory is in the critical ignore set
 */
function isCriticalIgnored(dirName: string): boolean {
  return CRITICAL_IGNORE_DIRS.has(dirName);
}

/**
 * Ignore Controller class
 */
export class IgnoreController {
  private cwd: string;
  private mode: IgnoreMode;
  private customIgnoreFile: string;
  private customPatterns: string[];
  private builtinPatterns: string[];
  private builtinIgnoreChecker: ((path: string) => boolean) | null = null;
  private gitignoreChecker: ((path: string) => boolean) | null = null;
  private customIgnoreChecker: ((path: string) => boolean) | null = null;
  private customIgnoreContent: string | undefined;
  private gitignoreContent: string | undefined;

  constructor(config: IgnoreControllerConfig) {
    this.cwd = config.cwd;
    this.mode = config.mode ?? IgnoreMode.All;
    this.customIgnoreFile = config.customIgnoreFile ?? ".agentignore";
    this.customPatterns = config.customPatterns ?? [];
    this.builtinPatterns = [];

    // Initialize built-in patterns
    this.initializeBuiltinPatterns();
  }

  /**
   * Initialize built-in ignore patterns
   */
  private initializeBuiltinPatterns(): void {
    const patterns: string[] = [];

    for (const dir of BUILTIN_IGNORE_DIRS) {
      if (dir === ".*") {
        // Hidden directories pattern
        patterns.push("**/.*/**");
      } else if (dir.includes("/")) {
        // Path patterns
        patterns.push(`**/${dir}/**`);
      } else {
        // Simple directory names
        patterns.push(dir);
        patterns.push(`**/${dir}/**`);
      }
    }

    this.builtinPatterns = patterns;
    this.builtinIgnoreChecker = fastIgnore(patterns.join("\n"));
  }

  /**
   * Initialize the controller by loading ignore files
   */
  async initialize(): Promise<void> {
    await this.loadIgnoreFiles();
  }

  /**
   * Load ignore files based on mode
   */
  private async loadIgnoreFiles(): Promise<void> {
    // Reset checkers
    this.gitignoreChecker = null;
    this.customIgnoreChecker = null;

    // Load .gitignore if needed
    if (this.shouldUseGitignore()) {
      await this.loadGitignore();
    }

    // Load custom ignore file if needed
    if (this.shouldUseCustom()) {
      await this.loadCustomIgnore();
    }
  }

  /**
   * Check if gitignore should be used based on mode
   */
  private shouldUseGitignore(): boolean {
    return (
      this.mode === IgnoreMode.Gitignore ||
      this.mode === IgnoreMode.BuiltinAndGitignore ||
      this.mode === IgnoreMode.All
    );
  }

  /**
   * Check if custom ignore should be used based on mode
   */
  private shouldUseCustom(): boolean {
    return (
      this.mode === IgnoreMode.Custom ||
      this.mode === IgnoreMode.BuiltinAndCustom ||
      this.mode === IgnoreMode.All
    );
  }

  /**
   * Check if built-in patterns should be used based on mode
   */
  private shouldUseBuiltin(): boolean {
    return (
      this.mode === IgnoreMode.Builtin ||
      this.mode === IgnoreMode.BuiltinAndGitignore ||
      this.mode === IgnoreMode.BuiltinAndCustom ||
      this.mode === IgnoreMode.All
    );
  }

  /**
   * Load .gitignore file
   */
  private async loadGitignore(): Promise<void> {
    try {
      const gitignorePath = path.join(this.cwd, ".gitignore");
      const exists = await this.fileExists(gitignorePath);

      if (exists) {
        const content = await fs.readFile(gitignorePath, "utf8");
        this.gitignoreContent = content;
        this.gitignoreChecker = fastIgnore(content);
      } else {
        this.gitignoreContent = undefined;
        this.gitignoreChecker = null;
      }
    } catch (error) {
      console.error("Error loading .gitignore:", error);
      this.gitignoreContent = undefined;
      this.gitignoreChecker = null;
    }
  }

  /**
   * Load custom ignore file
   */
  private async loadCustomIgnore(): Promise<void> {
    try {
      const customPath = path.join(this.cwd, this.customIgnoreFile);
      const exists = await this.fileExists(customPath);

      if (exists) {
        const content = await fs.readFile(customPath, "utf8");
        this.customIgnoreContent = content;
        // Combine file content with custom patterns and the ignore file itself
        const allPatterns = [content, this.customIgnoreFile, ...this.customPatterns].join("\n");
        this.customIgnoreChecker = fastIgnore(allPatterns);
      } else {
        this.customIgnoreContent = undefined;
        // Still create checker from custom patterns if any
        if (this.customPatterns.length > 0) {
          this.customIgnoreChecker = fastIgnore(this.customPatterns.join("\n"));
        } else {
          this.customIgnoreChecker = null;
        }
      }
    } catch (error) {
      console.error(`Error loading ${this.customIgnoreFile}:`, error);
      this.customIgnoreContent = undefined;
      // Still create checker from custom patterns if any
      if (this.customPatterns.length > 0) {
        this.customIgnoreChecker = fastIgnore(this.customPatterns.join("\n"));
      } else {
        this.customIgnoreChecker = null;
      }
    }
  }

  /**
   * Check if a file exists
   */
  private async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Validate if a file path should be accessible
   * @param filePath - Path to check (can be relative or absolute)
   * @returns true if file is accessible, false if ignored
   */
  validateAccess(filePath: string): boolean {
    try {
      const absolutePath = path.resolve(this.cwd, filePath);

      // Check for special directories
      if (isSpecialDirectory(absolutePath)) {
        return false;
      }

      // Follow symlinks to get the real path
      let realPath: string;
      try {
        realPath = fsSync.realpathSync(absolutePath);
      } catch {
        // If realpath fails, use the original path
        realPath = absolutePath;
      }

      // Convert to relative path
      const relativePath = path.relative(this.cwd, realPath).replace(/\\/g, "/");

      // Check built-in patterns
      if (this.shouldUseBuiltin() && this.builtinIgnoreChecker) {
        if (this.builtinIgnoreChecker(relativePath)) {
          return false;
        }
      }

      // Check gitignore patterns
      if (this.shouldUseGitignore() && this.gitignoreChecker) {
        if (this.gitignoreChecker(relativePath)) {
          return false;
        }
      }

      // Check custom patterns
      if (this.shouldUseCustom() && this.customIgnoreChecker) {
        if (this.customIgnoreChecker(relativePath)) {
          return false;
        }
      }

      return true;
    } catch (error) {
      // Allow access on errors (fail open for usability)
      console.error("Error validating access:", error);
      return true;
    }
  }

  /**
   * Check if a directory should be included in listings
   * @param dirName - Directory name
   * @param fullPath - Full path to the directory
   * @param isTargetDir - Whether this is the explicitly targeted directory
   * @param insideExplicitTarget - Whether we're inside an explicitly targeted hidden directory
   */
  shouldIncludeDirectory(
    dirName: string,
    fullPath: string,
    isTargetDir: boolean = false,
    insideExplicitTarget: boolean = false
  ): boolean {
    // If this is the explicitly targeted directory, allow it
    if (isTargetDir) {
      // Only block critical directories
      return !isCriticalIgnored(dirName);
    }

    // If inside an explicitly targeted hidden directory
    if (insideExplicitTarget) {
      // Only block critical directories
      if (isCriticalIgnored(dirName)) {
        return false;
      }
      // Check against ignore patterns
      return this.validateAccess(fullPath);
    }

    // Regular directory inclusion logic
    // Check built-in patterns first
    if (this.shouldUseBuiltin() && isBuiltinIgnored(dirName)) {
      return false;
    }

    // Check against ignore patterns
    return this.validateAccess(fullPath);
  }

  /**
   * Filter an array of paths, removing those that should be ignored
   */
  filterPaths(paths: string[]): string[] {
    return paths.filter((p) => this.validateAccess(p));
  }

  /**
   * Get formatted instructions about ignore rules
   */
  getInstructions(): string | undefined {
    const parts: string[] = [];

    // Built-in patterns
    if (this.shouldUseBuiltin()) {
      const builtinList = BUILTIN_IGNORE_DIRS.filter((d) => d !== ".*").join(", ");
      parts.push(`# Built-in Ignore Patterns\n\nThe following directories are automatically ignored:\n${builtinList}`);
    }

    // Gitignore
    if (this.gitignoreContent && this.shouldUseGitignore()) {
      parts.push(`# .gitignore\n\n${this.gitignoreContent}`);
    }

    // Custom ignore
    if (this.customIgnoreContent && this.shouldUseCustom()) {
      parts.push(`# ${this.customIgnoreFile}\n\n${this.customIgnoreContent}`);
    }

    return parts.length > 0 ? parts.join("\n\n") : undefined;
  }

  /**
   * Get the current ignore mode
   */
  getMode(): IgnoreMode {
    return this.mode;
  }

  /**
   * Set the ignore mode
   */
  setMode(mode: IgnoreMode): void {
    if (this.mode !== mode) {
      this.mode = mode;
      this.loadIgnoreFiles();
    }
  }
}
