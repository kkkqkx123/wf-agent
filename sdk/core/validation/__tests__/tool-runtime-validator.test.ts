import { describe, it, expect, beforeEach } from "vitest";
import { RuntimeValidator } from "../tool-runtime-validator.js";
import type { Tool } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

function createTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "test_tool",
    type: "STATELESS",
    description: "A test tool",
    parameters: {
      type: "object",
      properties: {
        name: { type: "string", description: "Name" },
        age: { type: "integer", description: "Age" },
      },
      required: ["name"],
    },
    ...overrides,
  } as Tool;
}

describe("RuntimeValidator", () => {
  let validator: RuntimeValidator;

  beforeEach(() => {
    validator = new RuntimeValidator();
  });

  describe("validate", () => {
    it("should pass validation with valid parameters", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Alice", age: 30 })).not.toThrow();
    });

    it("should pass with only required parameters", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Bob" })).not.toThrow();
    });

    it("should throw RuntimeValidationError for missing required parameter", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, {})).toThrow(RuntimeValidationError);
    });

    it("should throw for extra parameter when strict mode", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Alice", extra: "value" })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should throw for wrong type (string instead of number)", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Alice", age: "thirty" })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should throw for non-integer when integer type expected", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Alice", age: 30.5 })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should handle optional parameter not provided", () => {
      const tool = createTool();
      expect(() => validator.validate(tool, { name: "Alice" })).not.toThrow();
    });

    it("should handle array parameter type", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            tags: { type: "array", description: "Tags", items: { type: "string" } },
          },
          required: [],
        },
      });
      expect(() => validator.validate(tool, { tags: ["a", "b", "c"] })).not.toThrow();
    });

    it("should reject non-array for array type", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            tags: { type: "array", description: "Tags", items: { type: "string" } },
          },
          required: [],
        },
      });
      expect(() => validator.validate(tool, { tags: "not-array" })).toThrow(RuntimeValidationError);
    });

    it("should validate string enum constraint", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            color: {
              type: "string",
              description: "Color",
              enum: ["red", "green", "blue"],
            },
          },
          required: [],
        },
      });
      expect(() => validator.validate(tool, { color: "red" })).not.toThrow();
      expect(() => validator.validate(tool, { color: "yellow" })).toThrow(RuntimeValidationError);
    });

    it("should validate string minLength", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            code: {
              type: "string",
              description: "Code",
              minLength: 3,
            },
          },
          required: ["code"],
        },
      });
      expect(() => validator.validate(tool, { code: "abc" })).not.toThrow();
      expect(() => validator.validate(tool, { code: "ab" })).toThrow(RuntimeValidationError);
    });

    it("should validate string maxLength", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            code: {
              type: "string",
              description: "Code",
              maxLength: 5,
            },
          },
          required: ["code"],
        },
      });
      expect(() => validator.validate(tool, { code: "12345" })).not.toThrow();
      expect(() => validator.validate(tool, { code: "123456" })).toThrow(RuntimeValidationError);
    });

    it("should validate string pattern", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            email: {
              type: "string",
              description: "Email",
              pattern: "^[a-z]+@[a-z]+\\.[a-z]+$",
            },
          },
          required: ["email"],
        },
      });
      expect(() => validator.validate(tool, { email: "test@example.com" })).not.toThrow();
      expect(() => validator.validate(tool, { email: "invalid" })).toThrow(RuntimeValidationError);
    });

    it("should validate number minimum", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            value: {
              type: "number",
              description: "Value",
              minimum: 10,
            },
          },
          required: ["value"],
        },
      });
      expect(() => validator.validate(tool, { value: 15 })).not.toThrow();
      expect(() => validator.validate(tool, { value: 5 })).toThrow(RuntimeValidationError);
    });

    it("should validate number maximum", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            value: {
              type: "number",
              description: "Value",
              maximum: 100,
            },
          },
          required: ["value"],
        },
      });
      expect(() => validator.validate(tool, { value: 50 })).not.toThrow();
      expect(() => validator.validate(tool, { value: 101 })).toThrow(RuntimeValidationError);
    });

    it("should validate boolean type", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            flag: { type: "boolean", description: "Flag" },
          },
          required: ["flag"],
        },
      });
      expect(() => validator.validate(tool, { flag: true })).not.toThrow();
      expect(() => validator.validate(tool, { flag: "yes" })).toThrow(RuntimeValidationError);
    });

    it("should handle nested object parameters", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            address: {
              type: "object",
              description: "Address",
              properties: {
                city: { type: "string" },
                zip: { type: "string" },
              },
              required: ["city"],
            },
          },
          required: ["address"],
        },
      });
      expect(() =>
        validator.validate(tool, { address: { city: "NYC", zip: "10001" } }),
      ).not.toThrow();
      expect(() => validator.validate(tool, { address: { zip: "10001" } })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should handle unknown property type with z.any()", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            data: { type: "any" as any, description: "Any data" },
          },
          required: [],
        },
      });
      expect(() => validator.validate(tool, { data: { anything: "goes" } })).not.toThrow();
    });

    it("should validate url format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            website: {
              type: "string",
              description: "Website",
              format: "url",
            },
          },
          required: ["website"],
        },
      });
      expect(() => validator.validate(tool, { website: "https://example.com" })).not.toThrow();
      expect(() => validator.validate(tool, { website: "not-a-url" })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should validate email format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            email: {
              type: "string",
              description: "Email",
              format: "email",
            },
          },
          required: ["email"],
        },
      });
      expect(() => validator.validate(tool, { email: "test@example.com" })).not.toThrow();
      expect(() => validator.validate(tool, { email: "not-an-email" })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should validate uuid format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            id: {
              type: "string",
              description: "UUID",
              format: "uuid",
            },
          },
          required: ["id"],
        },
      });
      expect(() =>
        validator.validate(tool, {
          id: "550e8400-e29b-41d4-a716-446655440000",
        }),
      ).not.toThrow();
      expect(() => validator.validate(tool, { id: "not-a-uuid" })).toThrow(RuntimeValidationError);
    });

    it("should validate date-time format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            timestamp: {
              type: "string",
              description: "Timestamp",
              format: "date-time",
            },
          },
          required: ["timestamp"],
        },
      });
      expect(() => validator.validate(tool, { timestamp: "2024-01-15T10:30:00Z" })).not.toThrow();
    });

    it("should validate date format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            date: {
              type: "string",
              description: "Date",
              format: "date",
            },
          },
          required: ["date"],
        },
      });
      expect(() => validator.validate(tool, { date: "2024-01-15" })).not.toThrow();
    });

    it("should validate time format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            time: {
              type: "string",
              description: "Time",
              format: "time",
            },
          },
          required: ["time"],
        },
      });
      expect(() => validator.validate(tool, { time: "10:30:00" })).not.toThrow();
    });

    it("should validate ipv4 format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            ip: {
              type: "string",
              description: "IP",
              format: "ipv4",
            },
          },
          required: ["ip"],
        },
      });
      expect(() => validator.validate(tool, { ip: "192.168.1.1" })).not.toThrow();
      expect(() => validator.validate(tool, { ip: "999.999.999.999" })).toThrow(
        RuntimeValidationError,
      );
    });

    it("should validate ipv6 format", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            ip: {
              type: "string",
              description: "IP",
              format: "ipv6",
            },
          },
          required: ["ip"],
        },
      });
      expect(() =>
        validator.validate(tool, { ip: "2001:0db8:85a3:0000:0000:8a2e:0370:7334" }),
      ).not.toThrow();
      expect(() => validator.validate(tool, { ip: "::1" })).not.toThrow();
    });

    it("should accept unknown format as any string", () => {
      const tool = createTool({
        parameters: {
          type: "object",
          properties: {
            custom: {
              type: "string",
              description: "Custom",
              format: "unknown-format",
            },
          },
          required: ["custom"],
        },
      });
      expect(() => validator.validate(tool, { custom: "anything" })).not.toThrow();
    });
  });
});
