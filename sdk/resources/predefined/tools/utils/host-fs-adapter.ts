/**
 * HostFSAdapter — Default VFSFileIO implementation backed by the host filesystem.
 *
 * This is the fallback VFS provider used when no SandboxVFS is configured.
 * It directly wraps Node.js fs/promises to implement the VFSFileIO interface,
 * allowing all filesystem tools to uniformly call vfs operations without
 * branching between "vfs mode" and "direct fs mode".
 *
 * Usage:
 *   const vfs = config.vfs ?? new HostFSAdapter();
 *   const content = await vfs.readFile("/path/to/file");
 */

import {
  readFile,
  writeFile,
  stat,
  mkdir,
  unlink,
  rename,
  readdir,
} from "fs/promises";
import { existsSync } from "fs";
import { basename, dirname } from "path";
import type { VFSFileIO } from "../types.js";

export class HostFSAdapter implements VFSFileIO {
  async readFile(path: string): Promise<Buffer | null> {
    try {
      return await readFile(path);
    } catch {
      return null;
    }
  }

  async writeFile(path: string, data: Buffer): Promise<void> {
    await writeFile(path, data);
  }

  async stat(
    path: string,
  ): Promise<{ name: string; type: "file" | "directory"; size: number } | null> {
    try {
      const s = await stat(path);
      return {
        name: basename(path),
        type: s.isDirectory() ? "directory" : "file",
        size: s.size,
      };
    } catch {
      return null;
    }
  }

  async exists(path: string): Promise<boolean> {
    return existsSync(path);
  }

  async mkdir(path: string): Promise<void> {
    await mkdir(dirname(path), { recursive: true });
    // Also ensure the leaf directory itself is created
    await mkdir(path, { recursive: true });
  }

  async remove(path: string): Promise<void> {
    await unlink(path);
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    await rename(oldPath, newPath);
  }

  async readdir(path: string): Promise<string[]> {
    const entries = await readdir(path, { withFileTypes: true });
    return entries.map((e) => e.name);
  }
}