import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { CheckpointStore } from "../checkpoint-store.js";

describe("CheckpointStore", () => {
  let store: CheckpointStore<string>;

  beforeEach(() => {
    vi.useFakeTimers();
    store = new CheckpointStore<string>({ ttl: 10000 });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("get and set", () => {
    it("should set and get a value", () => {
      store.set("key1", "value1");
      expect(store.get("key1")).toBe("value1");
    });

    it("should return null for non-existent key", () => {
      expect(store.get("nonexistent")).toBeNull();
    });

    it("should return null for expired entry", () => {
      store.set("key1", "value1");
      vi.advanceTimersByTime(10001);
      expect(store.get("key1")).toBeNull();
    });

    it("should return value if not yet expired", () => {
      store.set("key1", "value1");
      vi.advanceTimersByTime(9999);
      expect(store.get("key1")).toBe("value1");
    });

    it("should store metadata", () => {
      const metaStore = new CheckpointStore<{ data: string }>();
      metaStore.set("key1", { data: "value1" }, { source: "test" });
      // Cannot directly verify metadata, but value should be retrievable
      const value = metaStore.get("key1");
      expect(value).toEqual({ data: "value1" });
    });

    it("should overwrite existing value", () => {
      store.set("key1", "old");
      store.set("key1", "new");
      expect(store.get("key1")).toBe("new");
    });
  });

  describe("has", () => {
    it("should return true for existing key", () => {
      store.set("key1", "value1");
      expect(store.has("key1")).toBe(true);
    });

    it("should return false for non-existent key", () => {
      expect(store.has("nonexistent")).toBe(false);
    });

    it("should return false for expired key", () => {
      store.set("key1", "value1");
      vi.advanceTimersByTime(10001);
      expect(store.has("key1")).toBe(false);
    });
  });

  describe("delete", () => {
    it("should delete an existing key", () => {
      store.set("key1", "value1");
      expect(store.delete("key1")).toBe(true);
      expect(store.get("key1")).toBeNull();
    });

    it("should return false for non-existent key", () => {
      expect(store.delete("nonexistent")).toBe(false);
    });
  });

  describe("clear", () => {
    it("should clear all entries and reset stats", () => {
      store.set("key1", "value1");
      store.set("key2", "value2");
      store.get("key1"); // hit
      store.get("nonexistent"); // miss

      store.clear();
      expect(store.size).toBe(0);
      expect(store.getStats().hits).toBe(0);
      expect(store.getStats().misses).toBe(0);
    });
  });

  describe("size", () => {
    it("should return the number of entries", () => {
      expect(store.size).toBe(0);
      store.set("key1", "value1");
      expect(store.size).toBe(1);
      store.set("key2", "value2");
      expect(store.size).toBe(2);
    });
  });

  describe("getStats", () => {
    it("should return correct stats", () => {
      store.get("a"); // miss
      store.set("b", "val");
      store.get("b"); // hit
      store.get("c"); // miss

      const stats = store.getStats();
      expect(stats.entries).toBe(1);
      expect(stats.hits).toBe(1);
      expect(stats.misses).toBe(2);
      expect(stats.hitRate).toBeCloseTo(1 / 3);
    });

    it("should have hitRate 0 when no operations", () => {
      const stats = store.getStats();
      expect(stats.hitRate).toBe(0);
    });
  });

  describe("keys", () => {
    it("should return all keys", () => {
      store.set("a", "1");
      store.set("b", "2");
      const keys = store.keys();
      expect(keys).toContain("a");
      expect(keys).toContain("b");
      expect(keys.length).toBe(2);
    });

    it("should return empty array for empty store", () => {
      expect(store.keys()).toEqual([]);
    });
  });

  describe("cleanup", () => {
    it("should remove expired entries", () => {
      store.set("key1", "value1");
      store.set("key2", "value2");
      vi.advanceTimersByTime(5000);
      store.set("key3", "value3");

      vi.advanceTimersByTime(6000);

      const removed = store.cleanup();
      expect(removed).toBe(2);
      expect(store.size).toBe(1);
      expect(store.get("key3")).toBe("value3");
    });

    it("should return 0 when no expired entries", () => {
      store.set("key1", "value1");
      expect(store.cleanup()).toBe(0);
    });
  });

  describe("getOrSet", () => {
    it("should return cached value if exists", async () => {
      store.set("key1", "cached");
      const factory = vi.fn().mockResolvedValue("new");
      const result = await store.getOrSet("key1", factory);
      expect(result).toBe("cached");
      expect(factory).not.toHaveBeenCalled();
    });

    it("should call factory and cache result if not exists", async () => {
      const factory = vi.fn().mockResolvedValue("computed");
      const result = await store.getOrSet("key1", factory);
      expect(result).toBe("computed");
      expect(factory).toHaveBeenCalledOnce();
      expect(store.get("key1")).toBe("computed");
    });

    it("should re-fetch if entry is expired", async () => {
      store.set("key1", "old");
      vi.advanceTimersByTime(10001);

      const factory = vi.fn().mockResolvedValue("new");
      const result = await store.getOrSet("key1", factory);
      expect(result).toBe("new");
      expect(factory).toHaveBeenCalledOnce();
    });
  });

  describe("maxSize eviction", () => {
    it("should evict oldest entry when maxSize is reached", () => {
      const limitedStore = new CheckpointStore<string>({ maxSize: 2, ttl: 60000 });
      limitedStore.set("a", "1");
      limitedStore.set("b", "2");
      limitedStore.set("c", "3");

      expect(limitedStore.size).toBe(2);
      expect(limitedStore.get("a")).toBeNull();
      expect(limitedStore.get("b")).toBe("2");
      expect(limitedStore.get("c")).toBe("3");
    });

    it("should evict oldest when maxSize is 1", () => {
      const limitedStore = new CheckpointStore<string>({ maxSize: 1, ttl: 60000 });
      limitedStore.set("a", "1");
      limitedStore.set("b", "2");

      expect(limitedStore.size).toBe(1);
      expect(limitedStore.get("a")).toBeNull();
      expect(limitedStore.get("b")).toBe("2");
    });
  });

  describe("constructor defaults", () => {
    it("should use default TTL of 5 minutes", () => {
      const defaultStore = new CheckpointStore<string>();
      defaultStore.set("key1", "value1");

      vi.advanceTimersByTime(299999);
      expect(defaultStore.get("key1")).toBe("value1");

      vi.advanceTimersByTime(2);
      expect(defaultStore.get("key1")).toBeNull();
    });
  });
});