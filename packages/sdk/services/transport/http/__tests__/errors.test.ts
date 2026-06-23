import { describe, it, expect } from "vitest";
import { HttpError } from "@wf-agent/types";
import {
  BadRequestError,
  UnauthorizedError,
  ForbiddenError,
  NotFoundHttpError,
  ConflictError,
  UnprocessableEntityError,
  RateLimitError,
  InternalServerError,
  ServiceUnavailableError,
} from "../errors.js";

describe("HTTP Error Types", () => {
  describe("BadRequestError (400)", () => {
    it("should create with correct status code", () => {
      const err = new BadRequestError("Bad request");
      expect(err).toBeInstanceOf(HttpError);
      expect(err.statusCode).toBe(400);
      expect(err.message).toBe("Bad request");
    });
  });

  describe("UnauthorizedError (401)", () => {
    it("should create with correct status code", () => {
      const err = new UnauthorizedError("Unauthorized", { url: "/test" });
      expect(err.statusCode).toBe(401);
    });
  });

  describe("ForbiddenError (403)", () => {
    it("should create with correct status code", () => {
      const err = new ForbiddenError("Forbidden");
      expect(err.statusCode).toBe(403);
    });
  });

  describe("NotFoundHttpError (404)", () => {
    it("should create with url property", () => {
      const err = new NotFoundHttpError("Not found", "/api/resource", { url: "/api/resource" });
      expect(err.statusCode).toBe(404);
      expect(err.url).toBe("/api/resource");
    });
  });

  describe("ConflictError (409)", () => {
    it("should create with correct status code", () => {
      const err = new ConflictError("Conflict");
      expect(err.statusCode).toBe(409);
    });
  });

  describe("UnprocessableEntityError (422)", () => {
    it("should create with correct status code", () => {
      const err = new UnprocessableEntityError("Invalid data");
      expect(err.statusCode).toBe(422);
    });
  });

  describe("RateLimitError (429)", () => {
    it("should create with retryAfter", () => {
      const err = new RateLimitError("Rate limited", 30);
      expect(err.statusCode).toBe(429);
      expect(err.retryAfter).toBe(30);
    });

    it("should create without retryAfter", () => {
      const err = new RateLimitError("Rate limited");
      expect(err.retryAfter).toBeUndefined();
    });
  });

  describe("InternalServerError (500)", () => {
    it("should create with correct status code", () => {
      const err = new InternalServerError("Server error");
      expect(err.statusCode).toBe(500);
    });

    it("should accept cause", () => {
      const cause = new Error("Underlying DB error");
      const err = new InternalServerError("Server error", {}, cause);
      expect(err.cause).toBe(cause);
    });
  });

  describe("ServiceUnavailableError (503)", () => {
    it("should create with correct status code", () => {
      const err = new ServiceUnavailableError("Service unavailable");
      expect(err.statusCode).toBe(503);
    });
  });

  describe("HttpError hierarchy", () => {
    it("should all be instanceof HttpError", () => {
      const errors = [
        new BadRequestError("test"),
        new UnauthorizedError("test"),
        new ForbiddenError("test"),
        new NotFoundHttpError("test", "/"),
        new ConflictError("test"),
        new UnprocessableEntityError("test"),
        new RateLimitError("test"),
        new InternalServerError("test"),
        new ServiceUnavailableError("test"),
      ];
      for (const err of errors) {
        expect(err).toBeInstanceOf(HttpError);
        expect(err).toBeInstanceOf(Error);
      }
    });

    it("should have unique status codes", () => {
      const codes = [
        new BadRequestError("test").statusCode,
        new UnauthorizedError("test").statusCode,
        new ForbiddenError("test").statusCode,
        new NotFoundHttpError("test", "/").statusCode,
        new ConflictError("test").statusCode,
        new UnprocessableEntityError("test").statusCode,
        new RateLimitError("test").statusCode,
        new InternalServerError("test").statusCode,
        new ServiceUnavailableError("test").statusCode,
      ];
      expect(new Set(codes).size).toBe(codes.length);
    });
  });
});
