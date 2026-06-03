/**
 * Host Skill Loader
 *
 * Implementation of SkillFileLoader using the native fs/promises and path modules.
 * This is the default loader used in production (Node.js) environments.
 */

import * as fs from "fs/promises";
import * as path from "path";
import { Dirent } from "fs";
import type { SkillFileLoader, SkillDirectoryEntry } from "./types.js";

export class HostSkillLoader implements SkillFileLoader {
  async readDirectory(dirPath: string): Promise<SkillDirectoryEntry[]> {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    return entries.map((entry: Dirent) => ({
      name: entry.name,
      isDirectory: entry.isDirectory(),
    }));
  }

  async listFiles(dirPath: string): Promise<string[]> {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    return entries
      .filter((entry: Dirent) => entry.isFile())
      .map((entry: Dirent) => entry.name);
  }

  async exists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  async readTextFile(filePath: string): Promise<string> {
    return fs.readFile(filePath, "utf-8");
  }

  async readBinaryFile(filePath: string): Promise<Buffer> {
    return fs.readFile(filePath);
  }

  resolve(...segments: string[]): string {
    return path.resolve(...segments);
  }

  join(...segments: string[]): string {
    return path.join(...segments);
  }

  basename(filePath: string): string {
    return path.basename(filePath);
  }
}