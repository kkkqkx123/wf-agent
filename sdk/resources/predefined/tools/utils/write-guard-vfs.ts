/**
 * WriteGuardVFS — Composite VFSFileIO that splits read/write paths
 *
 * Architecture: Read operations bypass VFS (direct host FS access via
 * HostFSAdapter), while write operations are routed through the sandbox
 * VFS for policy enforcement (path whitelist checks).
 *
 * This separation ensures:
 *   - Read operations have zero overhead — no intermediate layer.
 *   - Write operations are guarded by path policy before execution.
 *   - External processes (compilation, shell commands) always see
 *     consistent file state since there is no hidden VFS layer.
 *
 * Usage:
 *   // When sandbox VFS is configured:
 *   const vfs = new WriteGuardVFS(new HostFSAdapter(), sandboxVFS);
 *
 *   // When no sandbox (reads and writes both go to host FS directly):
 *   const vfs = config.vfs ?? new HostFSAdapter();
 *
 * The VFSFileIO interface is the union of both paths. Tools always use
 * a single VFSFileIO instance and don't need to know which operations
 * go through which backend.
 */

import type { VFSFileIO } from "../types.js";
import { HostFSAdapter } from "./host-fs-adapter.js";

export class WriteGuardVFS implements VFSFileIO {
  private readIO: HostFSAdapter;
  private writeIO: VFSFileIO;

  /**
   * @param writeIO The VFS backend for write operations (e.g., SandboxVFS).
   *                Must support writeFile, mkdir, remove, rename.
   */
  constructor(writeIO: VFSFileIO) {
    this.readIO = new HostFSAdapter();
    this.writeIO = writeIO;
  }

  // =========================================================================
  // Read operations — always direct host FS access via HostFSAdapter
  // =========================================================================

  async readFile(path: string): Promise<Buffer | null> {
    return this.readIO.readFile(path);
  }

  async stat(
    path: string,
  ): Promise<{ name: string; type: "file" | "directory"; size: number } | null> {
    return this.readIO.stat(path);
  }

  async exists(path: string): Promise<boolean> {
    return this.readIO.exists(path);
  }

  async readdir(path: string): Promise<string[]> {
    return this.readIO.readdir(path);
  }

  // =========================================================================
  // Write operations — routed through VFS for policy enforcement
  // =========================================================================

  async writeFile(path: string, data: Buffer): Promise<void> {
    return this.writeIO.writeFile(path, data);
  }

  async mkdir(path: string): Promise<void> {
    return this.writeIO.mkdir(path);
  }

  async remove(path: string): Promise<void> {
    return this.writeIO.remove(path);
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    return this.writeIO.rename(oldPath, newPath);
  }
}
