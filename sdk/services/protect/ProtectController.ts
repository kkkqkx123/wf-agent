/**
 * Protect Controller
 *
 * Controls write access to sensitive configuration files by enforcing protection patterns.
 * Prevents auto-approved modifications to sensitive files.
 */

import * as path from "path";
import fastIgnore from "fast-ignore";

/**
 * Shield symbol for display
 */
export const SHIELD_SYMBOL = "\u{1F6E1}";

/**
 * Configuration options for ProtectController
 */
export interface ProtectControllerConfig {
  /** Working directory */
  cwd: string;
  /** Additional protected patterns */
  protectedPatterns?: string[];
}

/**
 * Predefined list of protected configuration patterns
 */
const DEFAULT_PROTECTED_PATTERNS = [
  // Agent configuration files
  ".agentignore",
  ".agentrules*",
  ".agentconfig",

  // IDE/Editor configuration
  ".vscode/**",
  ".idea/**",
  "*.code-workspace",

  // Version control
  ".git/**",
  ".gitignore",
  ".gitattributes",

  // Environment and secrets
  ".env",
  ".env.*",
  "*.pem",
  "*.key",

  // Build and package configuration
  "package.json",
  "package-lock.json",
  "pnpm-lock.yaml",
  "yarn.lock",
  "tsconfig.json",
  "turbo.json",

  // CI/CD configuration
  ".github/**",
  ".gitlab-ci.yml",
  "Jenkinsfile",

  // Documentation
  "README.md",
  "LICENSE",
  "CHANGELOG.md",
] as const;

/**
 * Protect Controller class
 */
export class ProtectController {
  private cwd: string;
  private ignoreChecker: (path: string) => boolean;
  private protectedPatterns: readonly string[];

  constructor(config: ProtectControllerConfig) {
    this.cwd = config.cwd;

    // Combine default and custom patterns
    this.protectedPatterns = config.protectedPatterns
      ? [...DEFAULT_PROTECTED_PATTERNS, ...config.protectedPatterns]
      : DEFAULT_PROTECTED_PATTERNS;

    // Initialize ignore checker with protected patterns
    this.ignoreChecker = fastIgnore(this.protectedPatterns.join("\n"));
  }

  /**
   * Check if a file is write-protected
   * @param filePath - Path to check (can be relative or absolute)
   * @returns true if file is write-protected, false otherwise
   */
  isWriteProtected(filePath: string): boolean {
    try {
      // Normalize path to be relative to cwd and use forward slashes
      const absolutePath = path.resolve(this.cwd, filePath);
      const relativePath = path.relative(this.cwd, absolutePath).replace(/\\/g, "/");

      // Use ignore checker to check if file matches any protected pattern
      return this.ignoreChecker(relativePath);
    } catch (error) {
      // If there's an error processing the path, err on the side of caution
      console.error(`Error checking protection for ${filePath}:`, error);
      return false;
    }
  }

  /**
   * Get set of write-protected files from a list
   * @param paths - Array of paths to check
   * @returns Set of protected file paths
   */
  getProtectedFiles(paths: string[]): Set<string> {
    const protectedFiles = new Set<string>();

    for (const filePath of paths) {
      if (this.isWriteProtected(filePath)) {
        protectedFiles.add(filePath);
      }
    }

    return protectedFiles;
  }

  /**
   * Filter an array of paths, marking which ones are protected
   * @param paths - Array of paths to check
   * @returns Array of objects with path and protection status
   */
  annotatePathsWithProtection(paths: string[]): Array<{ path: string; isProtected: boolean }> {
    return paths.map((filePath) => ({
      path: filePath,
      isProtected: this.isWriteProtected(filePath),
    }));
  }

  /**
   * Get display message for protected file operations
   */
  getProtectionMessage(): string {
    return "This file is write-protected and requires approval for modifications";
  }

  /**
   * Get formatted instructions about protected files
   */
  getInstructions(): string {
    const patterns = this.protectedPatterns.join(", ");
    return `# Protected Files\n\nThe following file patterns are write-protected and always require approval for modifications, regardless of auto-approval settings. When using list_files, you'll notice a ${SHIELD_SYMBOL} next to files that are write-protected.\n\nProtected patterns: ${patterns}`;
  }

  /**
   * Get the list of protected patterns
   */
  getProtectedPatterns(): readonly string[] {
    return this.protectedPatterns;
  }
}
