import { describe, it, expect, beforeEach } from "vitest";
import {
  CheckpointConfigResolver,
  shouldCreateCheckpoint,
  getCheckpointDescription,
  type ConfigLayer,
} from "../checkpoint-config-resolver.js";

class TestConfigResolver extends CheckpointConfigResolver {}

describe("CheckpointConfigResolver", () => {
  let resolver: TestConfigResolver;

  beforeEach(() => {
    resolver = new TestConfigResolver();
  });

  describe("resolve", () => {
    it("should return default values when no layers are provided", () => {
      const result = resolver.resolve([]);
      expect(result.shouldCreate).toBe(false);
      expect(result.description).toBe("Checkpoint");
      expect(result.effectiveSource).toBe("default");
    });

    it("should use defaultEnabled when no layers are defined", () => {
      const customResolver = new TestConfigResolver({ defaultEnabled: true });
      const result = customResolver.resolve([]);
      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("default");
    });

    it("should use custom defaultDescription", () => {
      const customResolver = new TestConfigResolver({ defaultDescription: "Custom desc" });
      const result = customResolver.resolve([]);
      expect(result.description).toBe("Custom desc");
    });

    it("should return the first layer with enabled defined (highest priority)", () => {
      const layers: ConfigLayer[] = [{ name: "runtime", enabled: true }, { name: "workflow" }];
      const result = resolver.resolve(layers);
      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("runtime");
    });

    it("should fall through to next layer when enabled is undefined", () => {
      const layers: ConfigLayer[] = [{ name: "runtime" }, { name: "workflow", enabled: false }];
      const result = resolver.resolve(layers);
      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("workflow");
    });

    it("should use description from the matched layer", () => {
      const layers: ConfigLayer[] = [
        { name: "workflow", enabled: true, description: "Workflow checkpoint" },
      ];
      const result = resolver.resolve(layers);
      expect(result.description).toBe("Workflow checkpoint");
    });

    it("should use defaultDescription when matched layer has no description", () => {
      const customResolver = new TestConfigResolver({ defaultDescription: "Fallback desc" });
      const layers: ConfigLayer[] = [{ name: "runtime", enabled: true }];
      const result = customResolver.resolve(layers);
      expect(result.description).toBe("Fallback desc");
    });

    it("should fall through all layers and use default when none have enabled defined", () => {
      const layers: ConfigLayer[] = [{ name: "runtime" }, { name: "workflow" }, { name: "node" }];
      const result = resolver.resolve(layers);
      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("default");
    });

    it("should handle mixed undefined and defined enabled values", () => {
      const layers: ConfigLayer[] = [
        { name: "runtime" },
        { name: "workflow", enabled: undefined },
        { name: "node", enabled: true },
      ];
      const result = resolver.resolve(layers);
      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("node");
    });
  });

  describe("createLayer", () => {
    it("should create a layer with just a name", () => {
      const layer = resolver["createLayer"]("runtime");
      expect(layer.name).toBe("runtime");
      expect(layer.enabled).toBeUndefined();
      expect(layer.description).toBeUndefined();
    });

    it("should create a layer with config", () => {
      const layer = resolver["createLayer"]("workflow", {
        enabled: true,
        description: "test",
      });
      expect(layer.name).toBe("workflow");
      expect(layer.enabled).toBe(true);
      expect(layer.description).toBe("test");
    });

    it("should create a layer with enabled only", () => {
      const layer = resolver["createLayer"]("node", { enabled: false });
      expect(layer.name).toBe("node");
      expect(layer.enabled).toBe(false);
      expect(layer.description).toBeUndefined();
    });
  });
});

describe("shouldCreateCheckpoint", () => {
  it("should return true when runtime layer enables checkpoint", () => {
    const resolver = new TestConfigResolver();
    const layers: ConfigLayer[] = [{ name: "runtime", enabled: true }];
    expect(shouldCreateCheckpoint(resolver, layers)).toBe(true);
  });

  it("should return false when all layers are undefined", () => {
    const resolver = new TestConfigResolver();
    expect(shouldCreateCheckpoint(resolver, [])).toBe(false);
  });
});

describe("getCheckpointDescription", () => {
  it("should return description from matched layer", () => {
    const resolver = new TestConfigResolver();
    const layers: ConfigLayer[] = [{ name: "workflow", enabled: true, description: "Workflow cp" }];
    expect(getCheckpointDescription(resolver, layers)).toBe("Workflow cp");
  });

  it("should return default description when no layer matches", () => {
    const resolver = new TestConfigResolver();
    expect(getCheckpointDescription(resolver, [])).toBe("Checkpoint");
  });
});
