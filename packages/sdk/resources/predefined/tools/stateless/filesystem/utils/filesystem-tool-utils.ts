/**
 * Filesystem Tool Utils
 *
 * Provides common utilities for filesystem-based tools (grep, list_files, glob).
 * Centralizes VFS, IgnoreController initialization and directory validation.
 */

import path from "path";
import type { VFSFileIO } from "../../../types.js";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";
import { IgnoreController } from "@wf-agent/sdk/services";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ component: "FilesystemToolUtils" });

/**
 * Filesystem tool context containing VFS and ignore controller
 */
export interface FilesystemToolContext {
  vfs: VFSFileIO;
  ignoreController?: IgnoreController;
}

/**
 * Filesystem Tool Utils class
 */
export class FilesystemToolUtils {
  /**
   * Check if a path is a special directory (root or home)
   */
  static isSpecialDirectory(dirPath: string): boolean {
    const absolutePath = path.resolve(dirPath);

    const root = process.platform === "win32" ? path.parse(absolutePath).root : "/";
    if (absolutePath === root) {
      return true;
    }

    const homeDir = process.env["HOME"] || process.env["USERPROFILE"];
    if (homeDir && absolutePath === homeDir) {
      return true;
    }

    return false;
  }

  /**
   * Initialize filesystem context with VFS and IgnoreController
   *
   * @param workspaceDir - Working directory
   * @param enableIgnore - Whether to enable ignore filtering (default: true)
   * @param includeIgnored - LLM override to include ignored files (default: false)
   * @param customVFS - Custom VFS implementation (optional)
   * @returns FilesystemToolContext with VFS and optional IgnoreController
   */
  static async initializeContext(
    workspaceDir: string,
    enableIgnore: boolean = true,
    includeIgnored: boolean = false,
    customVFS?: VFSFileIO,
  ): Promise<FilesystemToolContext> {
    const vfs = customVFS ?? new HostFSAdapter();

    // Determine if ignore controller should be initialized
    const useIgnore = includeIgnored !== true && enableIgnore;

    let ignoreController: IgnoreController | undefined;
    if (useIgnore) {
      ignoreController = new IgnoreController({
        cwd: workspaceDir,
        mode: "all",
      });

      try {
        await ignoreController.initialize();
      } catch (error) {
        logger.warn("Failed to initialize IgnoreController", { error });
        // Continue without ignore controller if initialization fails
        ignoreController = undefined;
      }
    }

    return {
      vfs,
      ignoreController,
    };
  }

  /**
   * Validate that a directory exists and is actually a directory
   *
   * @param dirPath - Directory path
   * @param vfs - VFS instance
   * @returns Validation result with optional error message
   */
  static async validateDirectory(
    dirPath: string,
    vfs: VFSFileIO,
  ): Promise<{ valid: boolean; error?: string }> {
    // Check for special directories
    if (this.isSpecialDirectory(dirPath)) {
      return {
        valid: false,
        error: `Access denied: Cannot access special directory '${dirPath}'`,
      };
    }

    // Check if directory exists and is actually a directory
    const dirStat = await vfs.stat(dirPath);
    if (!dirStat) {
      return {
        valid: false,
        error: `Directory not found: ${dirPath}`,
      };
    }

    if (dirStat.type !== "directory") {
      return {
        valid: false,
        error: `Not a directory: ${dirPath}`,
      };
    }

    return { valid: true };
  }

  /**
   * Check if a file or directory entry should be included based on ignore rules
   *
   * @param itemPath - Full path to the item
   * @param type - Type of the item (file or directory)
   * @param name - Name of the item (for directory checking)
   * @param ignoreController - IgnoreController instance (optional)
   * @returns Whether the item should be included
   */
  static shouldIncludeEntry(
    itemPath: string,
    type: "file" | "directory",
    name: string,
    ignoreController?: IgnoreController,
  ): boolean {
    if (!ignoreController) {
      return true;
    }

    if (type === "directory") {
      return ignoreController.shouldIncludeDirectory(name, itemPath);
    } else {
      return ignoreController.validateAccess(itemPath);
    }
  }

  /**
   * Cleanup resources (if needed)
   * Currently a no-op but provided for future extensibility
   */
  static async cleanup(_context: FilesystemToolContext): Promise<void> {
    // IgnoreController cleanup would go here if needed
    // For now, no-op
  }
}
