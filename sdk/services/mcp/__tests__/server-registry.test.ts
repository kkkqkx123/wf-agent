import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { McpServerRegistry } from "../server-registry.js";
import { McpConnectionManager } from "../connection-manager.js";

// Reset the singleton state before each test
beforeEach(() => {
  McpServerRegistry.reset();
});

afterEach(async () => {
  await McpServerRegistry.cleanup();
});

describe("McpServerRegistry", () => {
  describe("setClientInfo", () => {
    it("should set client info for new instances", () => {
      McpServerRegistry.setClientInfo({ name: "my-app", version: "2.0" });
      expect(McpServerRegistry.hasInstance()).toBe(false);
    });
  });

  describe("setOptions", () => {
    it("should set default options for new instances", () => {
      McpServerRegistry.setOptions({ connectionTimeout: 120000 });
      expect(McpServerRegistry.hasInstance()).toBe(false);
    });
  });

  describe("getInstance", () => {
    it("should create a singleton instance", async () => {
      const instance1 = await McpServerRegistry.getInstance();
      expect(instance1).toBeInstanceOf(McpConnectionManager);

      const instance2 = await McpServerRegistry.getInstance();
      expect(instance2).toBe(instance1); // Same instance
    });

    it("should increment refCount", async () => {
      expect(McpServerRegistry.getRefCount()).toBe(0);
      await McpServerRegistry.getInstance();
      expect(McpServerRegistry.getRefCount()).toBe(1);
    });
  });

  describe("release", () => {
    it("should decrement refCount", async () => {
      await McpServerRegistry.getInstance();
      expect(McpServerRegistry.getRefCount()).toBe(1);

      await McpServerRegistry.release();
      expect(McpServerRegistry.getRefCount()).toBe(0);
    });

    it("should cleanup instance when refCount reaches zero", async () => {
      await McpServerRegistry.getInstance();
      await McpServerRegistry.release();

      expect(McpServerRegistry.hasInstance()).toBe(false);
    });

    it("should not cleanup when multiple references exist", async () => {
      await McpServerRegistry.getInstance();
      await McpServerRegistry.getInstance();
      expect(McpServerRegistry.getRefCount()).toBe(2);

      await McpServerRegistry.release();
      expect(McpServerRegistry.hasInstance()).toBe(true);
      expect(McpServerRegistry.getRefCount()).toBe(1);
    });
  });

  describe("hasInstance", () => {
    it("should return false initially", () => {
      expect(McpServerRegistry.hasInstance()).toBe(false);
    });

    it("should return true after getInstance", async () => {
      await McpServerRegistry.getInstance();
      expect(McpServerRegistry.hasInstance()).toBe(true);
    });

    it("should return false after cleanup", async () => {
      await McpServerRegistry.getInstance();
      await McpServerRegistry.cleanup();
      expect(McpServerRegistry.hasInstance()).toBe(false);
    });
  });

  describe("cleanup", () => {
    it("should reset everything", async () => {
      await McpServerRegistry.getInstance();
      McpServerRegistry.setClientInfo({ name: "x", version: "1" });

      await McpServerRegistry.cleanup();
      expect(McpServerRegistry.hasInstance()).toBe(false);
      expect(McpServerRegistry.getRefCount()).toBe(0);
    });
  });
});

describe("getMcpManager / releaseMcpManager", () => {
  it("should get and release manager (convenience functions)", async () => {
    const { getMcpManager, releaseMcpManager } = await import("../server-registry.js");

    const manager = await getMcpManager();
    expect(manager).toBeInstanceOf(McpConnectionManager);

    await releaseMcpManager();
    expect(McpServerRegistry.getRefCount()).toBe(0);
  });
});
