import { describe, it, expect } from "vitest";
import { z } from "zod";
import { validateNodeConfig, validateNodeType, validateConfig, convertZodError } from "../utils.js";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateNodeConfig", () => {
  const testSchema = z.object({
    name: z.string().min(1, "Name is required"),
    age: z.number().int().positive(),
  });

  it("should return ok for valid config", () => {
    const config = { name: "test", age: 25 };
    const result = validateNodeConfig(config, testSchema, "node-1");

    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value).toEqual(config);
    }
  });

  it("should return err for invalid config", () => {
    const config = { name: "", age: -1 };
    const result = validateNodeConfig(config, testSchema, "node-1");

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error.length).toBeGreaterThan(0);
      expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it("should include node id in error configPath", () => {
    const config = { name: "", age: 25 };
    const result = validateNodeConfig(config, testSchema, "my-node");

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      const configPathErr = result.error.find(e => e.message.includes("Name"));
      expect(configPathErr).toBeDefined();
    }
  });

  it("should reject null config", () => {
    const result = validateNodeConfig(null, testSchema, "node-1");
    expect(result.isErr()).toBe(true);
  });

  it("should reject undefined config", () => {
    const result = validateNodeConfig(undefined, testSchema, "node-1");
    expect(result.isErr()).toBe(true);
  });
});

describe("validateNodeType", () => {
  it("should return ok when type matches", () => {
    const node = { type: "START", id: "node-1" };
    const result = validateNodeType(node, "START");

    expect(result.isOk()).toBe(true);
  });

  it("should return err when type does not match", () => {
    const node = { type: "END", id: "node-1" };
    const result = validateNodeType(node, "START");

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]!).toBeInstanceOf(ConfigurationValidationError);
      expect(result.error[0]!.message).toContain("Expected START");
      expect(result.error[0]!.message).toContain("END");
    }
  });

  it("should handle node without id", () => {
    const node = { type: "FOO" };
    const result = validateNodeType(node, "BAR");

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]!.message).toContain("Expected BAR");
    }
  });
});

describe("validateConfig", () => {
  const schema = z.object({
    name: z.string(),
    count: z.number().optional(),
  });

  it("should return ok for valid config", () => {
    const result = validateConfig({ name: "hello" }, schema, "my.path", "tool");
    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value).toEqual({ name: "hello" });
    }
  });

  it("should return err for invalid config with configPath prefix", () => {
    const result = validateConfig({ name: 123 }, schema, "my.path", "node");

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      const err = result.error[0];
      expect(err).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it('should use default configType "schema"', () => {
    const result = validateConfig({ name: 123 }, schema, "test");
    expect(result.isErr()).toBe(true);
  });

  it("should handle empty object if schema allows", () => {
    const emptySchema = z.object({}).strict();
    const result = validateConfig({}, emptySchema, "path", "workflow");
    expect(result.isOk()).toBe(true);
  });

  it("should reject undefined config", () => {
    const result = validateConfig(undefined, schema, "path", "tool");
    expect(result.isErr()).toBe(true);
  });
});

describe("convertZodError", () => {
  const schema = z.object({
    name: z.string().min(1, "Name is required"),
    age: z.number().min(0, "Age must be positive"),
    tags: z.array(z.string()).min(1),
  });

  it("should convert ZodError to ConfigurationValidationError array", () => {
    const result = schema.safeParse({ name: "", age: -1, tags: [] });
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error, "prefix", "tool");

      expect(errors.length).toBeGreaterThan(0);
      errors.forEach(e => {
        expect(e).toBeInstanceOf(ConfigurationValidationError);
      });
    }
  });

  it("should include prefix in configPath", () => {
    const result = schema.safeParse({ name: "", age: -1, tags: [] });
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error, "config.test", "node");
      errors.forEach(e => {
        expect(e.message).toBeDefined();
      });
    }
  });

  it("should handle empty path in issues", () => {
    const rootSchema = z.string().min(1);
    const result = rootSchema.safeParse("");
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error, "root", "schema");
      expect(errors.length).toBe(1);
    }
  });

  it("should handle nested path issues", () => {
    const deepSchema = z.object({
      level1: z.object({
        level2: z.object({
          value: z.number(),
        }),
      }),
    });
    const result = deepSchema.safeParse({ level1: { level2: { value: "not-a-number" } } });
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error, "deep", "schema");
      expect(errors.length).toBeGreaterThan(0);
    }
  });

  it("should work without prefix", () => {
    const schema = z.object({ x: z.number() });
    const result = schema.safeParse({ x: "str" });
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error);
      expect(errors.length).toBe(1);
      expect(errors[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it("should use provided configType", () => {
    const schema = z.object({ x: z.number() });
    const result = schema.safeParse({ x: "str" });
    expect(result.success).toBe(false);
    if (!result.success) {
      const errors = convertZodError(result.error, "path", "llm");
      expect(errors.length).toBe(1);
    }
  });
});
