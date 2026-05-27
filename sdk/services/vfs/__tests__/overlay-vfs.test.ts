import { describe, it, expect, beforeEach } from "vitest";
import { OverlayVFS } from "../overlay-vfs.js";
import type { VFSConfig } from "@wf-agent/types";

function createVFSConfig(overrides?: Partial<VFSConfig>): VFSConfig {
  return {
    enabled: true,
    storage: "memory",
    workspaceRoot: "/test/workspace",
    ...overrides,
  };
}

describe("OverlayVFS", () => {
  let vfs: OverlayVFS;

  beforeEach(() => {
    vfs = new OverlayVFS(createVFSConfig());
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

      const names = entries.map((e) => e.name).sort();
      expect(names).toEqual(["file1.txt", "file2.txt"]);
    });

    it("should remove a directory", async () => {
      await vfs.mkdir("/emptydir");
      await vfs.rmdir("/emptydir");
      expect(await vfs.exists("/emptydir")).toBe(false);
    });
  });

  describe("snapshot operations", () => {
    it("should create and restore snapshots", async () => {
      await vfs.writeFile("/snap-test.txt", Buffer.from("before"));

      const snapId = await vfs.snapshot();
      expect(snapId).toBeDefined();
      expect(snapId.length).toBeGreaterThan(0);

      await vfs.writeFile("/snap-test.txt", Buffer.from("after"));

      await vfs.restore(snapId);

      const data = await vfs.readFile("/snap-test.txt");
      expect(data).not.toBeNull();
      expect(data!.toString()).toBe("before");
    });

    it("should restore to state without files created after snapshot", async () => {
      await vfs.writeFile("/persistent.txt", Buffer.from("keep"));

      const snapId = await vfs.snapshot();

      await vfs.writeFile("/ephemeral.txt", Buffer.from("discard"));

      await vfs.restore(snapId);

      const persistent = await vfs.readFile("/persistent.txt");
      expect(persistent).not.toBeNull();

      const ephemeral = await vfs.readFile("/ephemeral.txt");
      expect(ephemeral).toBeNull();
    });
  });

  describe("write and whiteout interaction", () => {
    it("should clear whiteout when writing to a deleted path", async () => {
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