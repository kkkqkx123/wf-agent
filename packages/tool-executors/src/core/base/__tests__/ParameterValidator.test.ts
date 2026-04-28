/**
 * ParameterValidator Unit Tests
 */

import { describe, it, expect } from "vitest";
import { ParameterValidator } from "../ParameterValidator.js";
import type { Tool } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

describe("ParameterValidator", () => {
  const validator = new ParameterValidator();

  // Create auxiliary functions defined by the testing tool
  const createTool = (params: { properties: Record<string, any>; required?: string[] }): Tool => ({
    id: "test-tool",
    name: "Test Tool",
    type: "STATELESS",
    description: "A test tool",
    parameters: {
      type: "object",
      properties: params.properties,
      required: params.required || [],
    },
  });

  describe("validate", () => {
    describe("Basic type validation", () => {
      it("The string type parameter should be validated.", () => {
        const tool = createTool({
          properties: {
            name: { type: "string" },
          },
          required: ["name"],
        });

        // Valid parameters
        expect(() => validator.validate(tool, { name: "test" })).not.toThrow();

        // Invalid parameter - Type error
        expect(() => validator.validate(tool, { name: 123 })).toThrow(RuntimeValidationError);
      });

      it("The numeric type parameters should be validated.", () => {
        const tool = createTool({
          properties: {
            count: { type: "number" },
          },
          required: ["count"],
        });

        expect(() => validator.validate(tool, { count: 42 })).not.toThrow();
        expect(() => validator.validate(tool, { count: 3.14 })).not.toThrow();
        expect(() => validator.validate(tool, { count: "42" })).toThrow(RuntimeValidationError);
      });

      it("The boolean type parameters should be validated.", () => {
        const tool = createTool({
          properties: {
            enabled: { type: "boolean" },
          },
          required: ["enabled"],
        });

        expect(() => validator.validate(tool, { enabled: true })).not.toThrow();
        expect(() => validator.validate(tool, { enabled: false })).not.toThrow();
        expect(() => validator.validate(tool, { enabled: "true" })).toThrow(RuntimeValidationError);
      });

      it("The array type parameter should be verified.", () => {
        const tool = createTool({
          properties: {
            items: { type: "array" },
          },
          required: ["items"],
        });

        expect(() => validator.validate(tool, { items: [1, 2, 3] })).not.toThrow();
        expect(() => validator.validate(tool, { items: [] })).not.toThrow();
        expect(() => validator.validate(tool, { items: "not-array" })).toThrow(
          RuntimeValidationError,
        );
      });

      it("The object type parameter should be verified.", () => {
        const tool = createTool({
          properties: {
            config: { type: "object" },
          },
          required: ["config"],
        });

        expect(() => validator.validate(tool, { config: { key: "value" } })).not.toThrow();
        expect(() => validator.validate(tool, { config: {} })).not.toThrow();
        expect(() => validator.validate(tool, { config: "not-object" })).toThrow(
          RuntimeValidationError,
        );
      });
    });

    describe("Required parameter validation", () => {
      it("An error should be thrown when the required parameters are missing.", () => {
        const tool = createTool({
          properties: {
            name: { type: "string" },
            age: { type: "number" },
          },
          required: ["name", "age"],
        });

        // Missing all required parameters
        expect(() => validator.validate(tool, {})).toThrow(RuntimeValidationError);

        // Missing some required parameters.
        expect(() => validator.validate(tool, { name: "test" })).toThrow(RuntimeValidationError);

        // All required parameters are present.
        expect(() => validator.validate(tool, { name: "test", age: 25 })).not.toThrow();
      });

      it("Optional parameters should be allowed to be omitted.", () => {
        const tool = createTool({
          properties: {
            name: { type: "string" },
            nickname: { type: "string" },
          },
          required: ["name"],
        });

        expect(() => validator.validate(tool, { name: "test" })).not.toThrow();
        expect(() => validator.validate(tool, { name: "test", nickname: "nick" })).not.toThrow();
      });
    });

    describe("Enumeration validation", () => {
      it("The enumeration values should be verified.", () => {
        const tool = createTool({
          properties: {
            status: {
              type: "string",
              enum: ["active", "inactive", "pending"],
            },
          },
          required: ["status"],
        });

        expect(() => validator.validate(tool, { status: "active" })).not.toThrow();
        expect(() => validator.validate(tool, { status: "inactive" })).not.toThrow();
        expect(() => validator.validate(tool, { status: "invalid" })).toThrow(
          RuntimeValidationError,
        );
      });
    });

    describe("Format Validation", () => {
      it("The URI format should be verified.", () => {
        const tool = createTool({
          properties: {
            url: { type: "string", format: "uri" },
          },
          required: ["url"],
        });

        expect(() => validator.validate(tool, { url: "https://example.com" })).not.toThrow();
        expect(() => validator.validate(tool, { url: "http://localhost:3000" })).not.toThrow();
        expect(() => validator.validate(tool, { url: "not-a-url" })).toThrow(
          RuntimeValidationError,
        );
      });

      it("The email format should be validated.", () => {
        const tool = createTool({
          properties: {
            email: { type: "string", format: "email" },
          },
          required: ["email"],
        });

        expect(() => validator.validate(tool, { email: "test@example.com" })).not.toThrow();
        expect(() => validator.validate(tool, { email: "invalid-email" })).toThrow(
          RuntimeValidationError,
        );
      });

      it("The UUID format should be validated.", () => {
        const tool = createTool({
          properties: {
            id: { type: "string", format: "uuid" },
          },
          required: ["id"],
        });

        expect(() =>
          validator.validate(tool, { id: "123e4567-e89b-12d3-a456-426614174000" }),
        ).not.toThrow();
        expect(() => validator.validate(tool, { id: "not-a-uuid" })).toThrow(
          RuntimeValidationError,
        );
      });

      it("The date-time format should be verified.", () => {
        const tool = createTool({
          properties: {
            timestamp: { type: "string", format: "date-time" },
          },
          required: ["timestamp"],
        });

        expect(() => validator.validate(tool, { timestamp: "2024-01-01T00:00:00Z" })).not.toThrow();
        expect(() =>
          validator.validate(tool, { timestamp: "2024-01-01T00:00:00.000Z" }),
        ).not.toThrow();
        expect(() => validator.validate(tool, { timestamp: "not-a-datetime" })).toThrow(
          RuntimeValidationError,
        );
      });
    });

    describe("Complex parameter validation", () => {
      it("Multiple parameters should be verified.", () => {
        const tool = createTool({
          properties: {
            name: { type: "string" },
            age: { type: "number" },
            email: { type: "string", format: "email" },
            active: { type: "boolean" },
          },
          required: ["name", "age"],
        });

        expect(() =>
          validator.validate(tool, {
            name: "John",
            age: 30,
            email: "john@example.com",
            active: true,
          }),
        ).not.toThrow();

        expect(() =>
          validator.validate(tool, {
            name: "John",
            age: 30,
          }),
        ).not.toThrow();
      });

      it("Detailed error information should be provided when the verification fails.", () => {
        const tool = createTool({
          properties: {
            email: { type: "string", format: "email" },
          },
          required: ["email"],
        });

        try {
          validator.validate(tool, { email: "invalid" });
          expect.fail("Should have thrown");
        } catch (error) {
          expect(error).toBeInstanceOf(RuntimeValidationError);
          expect((error as RuntimeValidationError).message).toContain("email");
        }
      });
    });

    describe("Boundary cases", () => {
      it("The empty parameter object should be handled accordingly.", () => {
        const tool = createTool({
          properties: {},
          required: [],
        });

        expect(() => validator.validate(tool, {})).not.toThrow();
      });

      it("Unknown types should be handled (use 'any').", () => {
        const tool = createTool({
          properties: {
            data: { type: "unknown-type" as any },
          },
          required: ["data"],
        });

        // The unknown type should accept any value.
        expect(() => validator.validate(tool, { data: "anything" })).not.toThrow();
        expect(() => validator.validate(tool, { data: 123 })).not.toThrow();
        expect(() => validator.validate(tool, { data: null })).not.toThrow();
      });

      it("The unknown format should be handled accordingly.", () => {
        const tool = createTool({
          properties: {
            custom: { type: "string", format: "custom-format" },
          },
          required: ["custom"],
        });

        // The unknown format should accept any string.
        expect(() => validator.validate(tool, { custom: "any-string" })).not.toThrow();
      });
    });
  });
});
