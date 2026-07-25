import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import { ConfigWatcher } from "../config-watcher.js";

describe("ConfigWatcher", () => {
  let watcher: ConfigWatcher;
  let statSyncSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.useFakeTimers();
    watcher = new ConfigWatcher();
    statSyncSpy = vi.spyOn(fs, "statSync");
  });

  afterEach(() => {
    watcher.unwatch();
    vi.useRealTimers();
  });

  it("should start with no watched paths", () => {
    expect(watcher["watching"]).toBe(false);
    expect(watcher["paths"].length).toBe(0);
  });

  it("should watch a config file", () => {
    const mtime = Date.now();
    statSyncSpy.mockReturnValue({ mtimeMs: mtime } as fs.Stats);
    const cb = vi.fn();
    watcher.watch("/path/to/config.json", cb);
    expect(watcher["watching"]).toBe(true);
    expect(watcher["paths"]).toContain("/path/to/config.json");
  });

  it("should call callback when mtime changes", () => {
    const cb = vi.fn();
    statSyncSpy.mockImplementation(() => ({ mtimeMs: 1000 } as fs.Stats));
    watcher.watch("/path/to/config.json", cb);
    vi.advanceTimersByTime(500);
    expect(cb).toHaveBeenCalledTimes(1);

    statSyncSpy.mockImplementation(() => ({ mtimeMs: 2000 } as fs.Stats));
    vi.advanceTimersByTime(500);
    expect(cb).toHaveBeenCalledTimes(2);
  });

  it("should not call callback if mtime is unchanged", () => {
    const cb = vi.fn();
    statSyncSpy.mockImplementation(() => ({ mtimeMs: 1000 } as fs.Stats));
    watcher.watch("/path/to/config.json", cb);
    vi.advanceTimersByTime(500);
    const firstCalls = cb.mock.calls.length;
    vi.advanceTimersByTime(500);
    expect(cb.mock.calls.length).toBe(firstCalls);
  });

  it("should handle stat errors silently", () => {
    statSyncSpy.mockImplementation(() => { throw new Error("ENOENT"); });
    const cb = vi.fn();
    watcher.watch("/path/to/missing.json", cb);
    vi.advanceTimersByTime(500);
    expect(cb).not.toHaveBeenCalled();
  });

  it("should add same path only once", () => {
    const cb = vi.fn();
    watcher.watch("/path/to/config.json", cb);
    watcher.watch("/path/to/config.json", cb);
    expect(watcher["paths"].length).toBe(1);
  });

  it("should stop watching on unwatch", () => {
    statSyncSpy.mockReturnValue({ mtimeMs: 1000 } as fs.Stats);
    const cb = vi.fn();
    watcher.watch("/path/to/config.json", cb);
    watcher.unwatch();
    expect(watcher["watching"]).toBe(false);
    expect(watcher["paths"].length).toBe(0);
    expect(watcher["pollTimer"]).toBeUndefined();
  });

  it("should be idempotent on unwatch", () => {
    watcher.unwatch();
    watcher.unwatch();
  });
});
