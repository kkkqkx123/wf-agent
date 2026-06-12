import { describe, it, expect, beforeEach, vi } from "vitest";
import { ScriptRegistry } from "../script-registry.js";
import type { Script } from "@wf-agent/types";

vi.mock("../../executors/script-executor.js", () => {
  const mockExecute = vi.fn().mockResolvedValue({
    success: true,
    scriptName: "test-script",
    stdout: "executed",
    executionTime: 10,
  });
  return {
    ScriptExecutor: vi.fn().mockImplementation(function () {
      return {
        execute: mockExecute,
        cleanup: vi.fn().mockResolvedValue(undefined),
      };
    }),
  };
});

function createValidScript(overrides: Partial<Script> = {}): Script {
  return {
    name: "test-script",
    description: "A test script",
    content: 'console.log("hello")',
    options: {
      timeout: 5000,
      retries: 0,
      retryDelay: 0,
    },
    enabled: true,
    ...overrides,
  } as Script;
}

describe("ScriptRegistry", () => {
  let registry: ScriptRegistry;

  beforeEach(() => {
    registry = new ScriptRegistry();
  });

  describe("registerScript", () => {
    it("should register a valid script", async () => {
      await registry.registerScript(createValidScript());
      expect(registry.hasScript("test-script")).toBe(true);
    });

    it("should throw if name already exists", async () => {
      await registry.registerScript(createValidScript());
      await expect(registry.registerScript(createValidScript())).rejects.toThrow("already exists");
    });

    it("should set enabled to true by default", async () => {
      const script = createValidScript({ enabled: undefined });
      await registry.registerScript(script);
      expect(registry.getScript("test-script").enabled).toBe(true);
    });
  });

  describe("registerScripts", () => {
    it("should register multiple scripts", async () => {
      const s1 = createValidScript({ name: "s1" });
      const s2 = createValidScript({ name: "s2" });
      await registry.registerScripts([s1, s2]);
      expect(registry.scriptCount()).toBe(2);
    });

    it("should throw on duplicate in batch", async () => {
      const s1 = createValidScript({ name: "s1" });
      const dup = createValidScript({ name: "s1" });
      await expect(registry.registerScripts([s1, dup])).rejects.toThrow("already exists");
    });
  });

  describe("unregisterScript", () => {
    it("should delete a script", async () => {
      await registry.registerScript(createValidScript());
      await registry.unregisterScript("test-script");
      expect(registry.hasScript("test-script")).toBe(false);
    });

    it("should throw if script does not exist", async () => {
      await expect(registry.unregisterScript("non-existent")).rejects.toThrow("not found");
    });
  });

  describe("getScript", () => {
    it("should return script by name", async () => {
      const script = createValidScript();
      await registry.registerScript(script);
      expect(registry.getScript("test-script").name).toBe("test-script");
    });

    it("should throw if script does not exist", () => {
      expect(() => registry.getScript("non-existent")).toThrow("not found");
    });
  });

  describe("findScript", () => {
    it("should return script or undefined", async () => {
      await registry.registerScript(createValidScript());
      expect(registry.findScript("test-script")).toBeDefined();
      expect(registry.findScript("non-existent")).toBeUndefined();
    });
  });

  describe("listScripts", () => {
    it("should return all scripts", async () => {
      await registry.registerScript(createValidScript({ name: "s1" }));
      await registry.registerScript(createValidScript({ name: "s2" }));
      expect(registry.listScripts()).toHaveLength(2);
    });
  });

  describe("listScriptsByCategory", () => {
    it("should filter by category", async () => {
      await registry.registerScript(
        createValidScript({ name: "s1", metadata: { category: "data" } }),
      );
      await registry.registerScript(
        createValidScript({ name: "s2", metadata: { category: "util" } }),
      );
      expect(registry.listScriptsByCategory("data")).toHaveLength(1);
    });
  });

  describe("searchScripts", () => {
    it("should search by name", async () => {
      await registry.registerScript(createValidScript({ name: "my-processor" }));
      expect(registry.searchScripts("processor")).toHaveLength(1);
    });

    it("should search by description", async () => {
      await registry.registerScript(
        createValidScript({ name: "s1", description: "data transformation" }),
      );
      expect(registry.searchScripts("transformation")).toHaveLength(1);
    });

    it("should search by tag", async () => {
      await registry.registerScript(
        createValidScript({ name: "s1", metadata: { tags: ["important"] } }),
      );
      expect(registry.searchScripts("important")).toHaveLength(1);
    });

    it("should search by category", async () => {
      await registry.registerScript(
        createValidScript({ name: "s1", metadata: { category: "machine-learning" } }),
      );
      expect(registry.searchScripts("machine")).toHaveLength(1);
    });

    it("should return empty for no match", async () => {
      await registry.registerScript(createValidScript({ name: "s1" }));
      expect(registry.searchScripts("nonexistent")).toHaveLength(0);
    });
  });

  describe("hasScript", () => {
    it("should return true if exists", async () => {
      await registry.registerScript(createValidScript());
      expect(registry.hasScript("test-script")).toBe(true);
    });

    it("should return false if not exists", () => {
      expect(registry.hasScript("non-existent")).toBe(false);
    });
  });

  describe("clearScripts", () => {
    it("should remove all scripts", async () => {
      await registry.registerScript(createValidScript({ name: "s1" }));
      await registry.registerScript(createValidScript({ name: "s2" }));
      registry.clearScripts();
      expect(registry.scriptCount()).toBe(0);
    });
  });

  describe("scriptCount", () => {
    it("should return correct count", async () => {
      expect(registry.scriptCount()).toBe(0);
      await registry.registerScript(createValidScript());
      expect(registry.scriptCount()).toBe(1);
    });
  });

  describe("updateScript", () => {
    it("should update existing script fields", async () => {
      await registry.registerScript(createValidScript());
      await registry.updateScript("test-script", { description: "Updated description" });
      expect(registry.getScript("test-script").description).toBe("Updated description");
    });

    it("should throw if script does not exist", async () => {
      await expect(registry.updateScript("non-existent", { description: "test" })).rejects.toThrow(
        "not found",
      );
    });
  });

  describe("enableScript / disableScript / isScriptEnabled", () => {
    it("should enable a script", async () => {
      await registry.registerScript(createValidScript({ enabled: false }));
      await registry.enableScript("test-script");
      expect(registry.isScriptEnabled("test-script")).toBe(true);
    });

    it("should disable a script", async () => {
      await registry.registerScript(createValidScript({ enabled: true }));
      await registry.disableScript("test-script");
      expect(registry.isScriptEnabled("test-script")).toBe(false);
    });

    it("should throw isScriptEnabled for non-existent script", () => {
      expect(() => registry.isScriptEnabled("non-existent")).toThrow("not found");
    });
  });

  describe("validateScript", () => {
    it("should throw on empty name", async () => {
      await expect(registry.registerScript(createValidScript({ name: "" }))).rejects.toThrow(
        "name is required",
      );
    });

    it("should throw on missing name", async () => {
      await expect(
        registry.registerScript(createValidScript({ name: undefined as any })),
      ).rejects.toThrow("name is required");
    });

    it("should throw on missing description", async () => {
      await expect(registry.registerScript(createValidScript({ description: "" }))).rejects.toThrow(
        "description is required",
      );
    });

    it("should throw when content, filePath, and template are all missing", async () => {
      await expect(
        registry.registerScript(
          createValidScript({ content: undefined, filePath: undefined, template: undefined }),
        ),
      ).rejects.toThrow("must have either content, filePath, or template");
    });

    it("should accept script with filePath instead of content", async () => {
      await registry.registerScript(
        createValidScript({ content: undefined, filePath: "/path/to/script.js" }),
      );
      expect(registry.hasScript("test-script")).toBe(true);
    });

    it("should accept script with template instead of content", async () => {
      await registry.registerScript(
        createValidScript({ content: undefined, template: "echo {{input}}" }),
      );
      expect(registry.hasScript("test-script")).toBe(true);
    });

    it("should throw on missing options", async () => {
      await expect(
        registry.registerScript(createValidScript({ options: undefined as any })),
      ).rejects.toThrow("options are required");
    });

    it("should throw on negative timeout", async () => {
      await expect(
        registry.registerScript(
          createValidScript({ options: { timeout: -1, retries: 0, retryDelay: 0 } as any }),
        ),
      ).rejects.toThrow("timeout");
    });

    it("should throw on negative retries", async () => {
      await expect(
        registry.registerScript(
          createValidScript({ options: { timeout: 1000, retries: -1, retryDelay: 0 } as any }),
        ),
      ).rejects.toThrow("retries");
    });

    it("should throw on negative retryDelay", async () => {
      await expect(
        registry.registerScript(
          createValidScript({ options: { timeout: 1000, retries: 0, retryDelay: -1 } as any }),
        ),
      ).rejects.toThrow("retryDelay");
    });

    it("should throw on invalid enabled type", async () => {
      await expect(
        registry.registerScript(createValidScript({ enabled: "yes" as any })),
      ).rejects.toThrow("enabled must be a boolean");
    });
  });

  describe("execute", () => {
    it("should execute a registered script", async () => {
      await registry.registerScript(createValidScript());
      const result = await registry.execute("test-script", {});
      expect(result.isOk()).toBe(true);
      const val = result.unwrap();
      expect(val.stdout).toBe("executed");
    });

    it("should throw for non-existent script", async () => {
      await expect(registry.execute("non-existent", {})).rejects.toThrow("not found");
    });
  });

  describe("executeFlow", () => {
    it("should throw when flow is not registered", async () => {
      await expect(registry.executeFlow("non-existent")).rejects.toThrow();
    });
  });
});
