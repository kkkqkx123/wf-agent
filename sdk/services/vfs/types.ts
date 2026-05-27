/**
 * VFS — Types
 *
 * Core interfaces for the Virtual File System layer.
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 */

import type { VFSConfig } from "@wf-agent/types";

export type { VFSConfig };

/**
 * Virtual file system entry metadata.
 */
export interface VFSEntry {
  name: string;
  path: string;
  type: "file" | "directory";
  size: number;
  mode: number;
  createdAt: number;
  modifiedAt: number;
}

/**
 * Core VFS operations.
 * All paths are absolute within the VFS namespace.
 */
export interface VFSOperations {
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  remove(path: string): Promise<void>;
  rename(oldPath: string, newPath: string): Promise<void>;

  readdir(path: string): Promise<VFSEntry[]>;
  mkdir(path: string): Promise<void>;
  rmdir(path: string): Promise<void>;

  exists(path: string): Promise<boolean>;
  isAllowed(path: string): boolean;
}

/**
 * Delta layer interface.
 * Each delta implementation (memory, SQLite) must implement this.
 */
export interface DeltaFileSystem {
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  remove(path: string): Promise<void>;
  rename(oldPath: string, newPath: string): Promise<void>;

  readdir(path: string): Promise<VFSEntry[]>;
  mkdir(path: string): Promise<void>;
  rmdir(path: string): Promise<void>;
}

/**
 * Base layer (read-only) interface.
 */
export interface BaseFileSystem {
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  readdir(path: string): Promise<VFSEntry[]>;
  exists(path: string): Promise<boolean>;
  isAllowed(path: string): boolean;
}