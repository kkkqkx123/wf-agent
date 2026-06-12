import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { SandboxVFS } from "../sandbox-vfs.js";
import type { VFSConfig } from "@wf-agent/types";

function createVFSConfig(workspaceRoot: string, overrides?: Partial<VFSConfig>): VFSConfig {
  return {
    enabled: true,
    workspaceRoot,
    ...overrides,
  };
}

describe("SandboxVFS", () => {
  let vfs: SandboxVFS;
  let workspaceRoot: string;

  beforeEach(() => {
    workspaceRoot = mkdtempSync(join(tmpdir(), "sandbox-vfs-test-"));
    vfs = new SandboxVFS(createVFSConfig(workspaceRoot));
  });

  afterEach(() => {
    rmSync(workspaceRoot, { recursive: true, force: true });
  });

  describe("file operations", () => {
    it("should write and read a file", async () => {
      await vfs.writeFile("/test.txt", Buffer.from("hello"));
      const data = await vfs.readFile("/test.txt");
      expect(data).not.toBeNull();
      expect(data!.toString()).toBe("hello");
    });

    it("should return null for non-existent file", async () => {
      const data = await vfs.readFile("/nonexistent.txt");
      expect(data).toBeNull();
    });

    it("should stat a written file", async () => {
      await vfs.writeFile("/test.txt", Buffer.from("hello"));
      const entry = await vfs.stat("/test.txt");
      expect(entry).not.toBeNull();
      expect(entry!.type).toBe("file");
      expect(entry!.name).toBe("test.txt");
      expect(entry!.size).toBe(5);
    });

    it("should remove a file", async () => {
      await vfs.writeFile("/test.txt", Buffer.from("hello"));
      await vfs.remove("/test.txt");
      const data = await vfs.readFile("/test.txt");
      expect(data).toBeNull();
    });

    it("should rename a file", async () => {
      await vfs.writeFile("/old.txt", Buffer.from("hello"));
      await vfs.rename("/old.txt", "/new.txt");

      const oldData = await vfs.readFile("/old.txt");
      expect(oldData).toBeNull();

      const newData = await vfs.readFile("/new.txt");
      expect(newData).not.toBeNull();
      expect(newData!.toString()).toBe("hello");
    });

    it("should check file existence", async () => {
      expect(await vfs.exists("/test.txt")).toBe(false);
      await vfs.writeFile("/test.txt", Buffer.from("hello"));
      expect(await vfs.exists("/test.txt")).toBe(true);
    });
  });

  describe("directory operations", () => {
    it("should create and list directories", async () => {
      await vfs.mkdir("/subdir");
      await vfs.writeFile("/subdir/file1.txt", Buffer.from("content1"));
      await vfs.writeFile("/subdir/file2.txt", Buffer.from("content2"));

      const entries = await vfs.readdir("/subdir");
      expect(entries.length).toBe(2);

      const names = entries.map(e => e.name).sort();
      expect(names).toEqual(["file1.txt", "file2.txt"]);
    });

    it("should remove a directory", async () => {
      await vfs.mkdir("/emptydir");
      await vfs.rmdir("/emptydir");
      expect(await vfs.exists("/emptydir")).toBe(false);
    });
  });

  describe("write and delete interaction", () => {
    it("should allow writing to a path after deletion", async () => {
      await vfs.writeFile("/test.txt", Buffer.from("original"));
      await vfs.remove("/test.txt");

      const afterRemove = await vfs.readFile("/test.txt");
      expect(afterRemove).toBeNull();

      await vfs.writeFile("/test.txt", Buffer.from("rewritten"));
      const data = await vfs.readFile("/test.txt");
      expect(data).not.toBeNull();
      expect(data!.toString()).toBe("rewritten");
    });
  });
});
