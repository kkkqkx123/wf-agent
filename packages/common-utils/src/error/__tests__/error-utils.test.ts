import { describe, it, expect } from "vitest";
import {
  getErrorMessage,
  normalizeError,
  isError,
  getErrorOrUndefined,
  getErrorOrNew,
  createAbortError,
  isAbortError,
} from "../error-utils.js";
import { AbortError, ThreadInterruptedException } from "@wf-agent/types";

describe("error-utils", () => {
  describe("getErrorMessage", () => {
    it("should return the message of the Error object", () => {
      const error = new Error("Test error message");
      expect(getErrorMessage(error)).toBe("Test error message");
    });

    it("should return the string directly", () => {
      const error = "String error message";
      expect(getErrorMessage(error)).toBe("String error message");
    });

    it('Should be null Returns "Unknown error"', () => {
      expect(getErrorMessage(null)).toBe("Unknown error");
    });

    it('应该为 undefined 返回 "Unknown error"', () => {
      expect(getErrorMessage(undefined)).toBe("Unknown error");
    });

    it("The message attribute of the object should be extracted", () => {
      const error = { message: "Object error message" };
      expect(getErrorMessage(error)).toBe("Object error message");
    });

    it("The object's toString method should be called", () => {
      const error = { toString: () => "Custom toString" };
      expect(getErrorMessage(error)).toBe("Custom toString");
    });

    it("The object's toString method should be called (normal objects return [object Object])", () => {
      const error = { code: 500, detail: "Server error" };
      expect(getErrorMessage(error)).toBe("[object Object]");
    });

    it("Numbers should be converted to strings", () => {
      expect(getErrorMessage(404)).toBe("404");
    });

    it("Boolean values should be converted to strings", () => {
      expect(getErrorMessage(true)).toBe("true");
    });
  });

  describe("normalizeError", () => {
    it("should return the Error object directly", () => {
      const error = new Error("Test error");
      const result = normalizeError(error);
      expect(result).toBe(error);
      expect(result.message).toBe("Test error");
    });

    it("The string should be converted to an Error object", () => {
      const error = "String error";
      const result = normalizeError(error);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("String error");
    });

    it('An Error object containing "Unknown error" should be created for null.', () => {
      const result = normalizeError(null);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("Unknown error");
    });

    it('An Error object containing "Unknown error" should be created for undefined.', () => {
      const result = normalizeError(undefined);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("Unknown error");
    });

    it("The Error should be created by extracting the message attribute of the object.", () => {
      const error = { message: "Object error" };
      const result = normalizeError(error);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("Object error");
    });

    it("The toString method of the object should be called to create the Error.", () => {
      const error = { toString: () => "Custom toString" };
      const result = normalizeError(error);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("Custom toString");
    });

    it("The toString method of the object should be called to create the Error (normal objects return [object Object])", () => {
      const error = { code: 500 };
      const result = normalizeError(error);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("[object Object]");
    });

    it("The number should be converted to an Error object", () => {
      const result = normalizeError(404);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("404");
    });
  });

  describe("isError", () => {
    it("should return true for the Error object", () => {
      const error = new Error("Test error");
      expect(isError(error)).toBe(true);
    });

    it("should return false for strings", () => {
      expect(isError("error string")).toBe(false);
    });

    it("Should be null Return false", () => {
      expect(isError(null)).toBe(false);
    });

    it("Should be undefined Returns false", () => {
      expect(isError(undefined)).toBe(false);
    });

    it("should return false for normal objects", () => {
      expect(isError({ message: "error" })).toBe(false);
    });

    it("Should return false as a number", () => {
      expect(isError(404)).toBe(false);
    });
  });

  describe("getErrorOrUndefined", () => {
    it("should return the Error object", () => {
      const error = new Error("Test error");
      const result = getErrorOrUndefined(error);
      expect(result).toBe(error);
    });

    it("should return undefined as a string", () => {
      const result = getErrorOrUndefined("error string");
      expect(result).toBeUndefined();
    });

    it("Should be null Returns undefined", () => {
      const result = getErrorOrUndefined(null);
      expect(result).toBeUndefined();
    });

    it("Should be undefined Returns undefined", () => {
      const result = getErrorOrUndefined(undefined);
      expect(result).toBeUndefined();
    });

    it("should return undefined for normal objects", () => {
      const result = getErrorOrUndefined({ message: "error" });
      expect(result).toBeUndefined();
    });
  });

  describe("getErrorOrNew", () => {
    it("should return the Error object", () => {
      const error = new Error("Test error");
      const result = getErrorOrNew(error);
      expect(result).toBe(error);
    });

    it("A new Error object should be created for the string", () => {
      const result = getErrorOrNew("error string");
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("error string");
    });

    it("A new Error object should be created for null.", () => {
      const result = getErrorOrNew(null);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("null");
    });

    it("A new Error object should be created for undefined", () => {
      const result = getErrorOrNew(undefined);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("undefined");
    });

    it("A new Error object should be created for the number", () => {
      const result = getErrorOrNew(404);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe("404");
    });
  });

  describe("createAbortError", () => {
    it("An instance of AbortError should be created", () => {
      const error = createAbortError("Operation aborted");
      expect(error).toBeInstanceOf(AbortError);
      expect(error.message).toBe("Operation aborted");
    });

    it("The reason of the signal should be used as the cause", () => {
      const controller = new AbortController();
      const reason = new Error("Custom abort reason");
      controller.abort(reason);

      const error = createAbortError("Operation aborted", controller.signal);
      expect(error.cause).toBe(reason);
    });

    it("AbortError without cause should be created when there is no signal.", () => {
      const error = createAbortError("Operation aborted");
      expect(error.cause).toBeUndefined();
    });
  });

  describe("isAbortError", () => {
    it("should return true for AbortError instances.", () => {
      const error = new AbortError("Operation aborted");
      expect(isAbortError(error)).toBe(true);
    });

    it("Should return true for nested AbortError", () => {
      const abortError = new AbortError("Operation aborted");
      const error = new Error("Wrapper error");
      error.cause = abortError;

      expect(isAbortError(error)).toBe(true);
    });

    it("Should return false for normal Error", () => {
      const error = new Error("Regular error");
      expect(isAbortError(error)).toBe(false);
    });

    it("should return false for strings", () => {
      expect(isAbortError("error string")).toBe(false);
    });

    it("Should be null Return false", () => {
      expect(isAbortError(null)).toBe(false);
    });

    it("Should be undefined Returns false", () => {
      expect(isAbortError(undefined)).toBe(false);
    });

    it("Should return false for nested non-AbortError", () => {
      const innerError = new Error("Inner error");
      const error = new Error("Wrapper error");
      error.cause = innerError;

      expect(isAbortError(error)).toBe(false);
    });
  });
});
