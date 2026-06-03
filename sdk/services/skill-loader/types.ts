/**
 * Skill File Loader Types
 *
 * Provides an abstraction layer for file I/O operations required by SkillRegistry.
 * Separates file system access from business logic to improve testability and portability.
 */

export interface SkillDirectoryEntry {
  name: string;
  isDirectory: boolean;
}

export interface SkillFileLoader {
  // ── Directory operations ──

  /** Read all entries in a directory */
  readDirectory(dirPath: string): Promise<SkillDirectoryEntry[]>;

  /** List file names in a directory (non-recursive) */
  listFiles(dirPath: string): Promise<string[]>;

  // ── File operations ──

  /** Check if a file or directory exists */
  exists(filePath: string): Promise<boolean>;

  /** Read a text file as UTF-8 string */
  readTextFile(filePath: string): Promise<string>;

  /** Read a binary file as Buffer */
  readBinaryFile(filePath: string): Promise<Buffer>;

  // ── Path operations ──

  /** Resolve a sequence of path segments into an absolute path */
  resolve(...segments: string[]): string;

  /** Join path segments using the platform-specific separator */
  join(...segments: string[]): string;

  /** Get the last portion (base name) of a path */
  basename(filePath: string): string;
}