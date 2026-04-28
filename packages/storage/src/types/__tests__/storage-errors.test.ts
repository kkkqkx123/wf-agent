/**
 * Storage Errors Tests
 */

import { describe, it, expect } from "vitest";
import {
  StorageError,
  StorageQuotaExceededError,
  EntityNotFoundError,
  StorageInitializationError,
  SerializationError,
} from "../storage-errors.js";

describe("StorageError", () => {
  it("should create error with message and operation", () => {
    const error = new StorageError("Test error", "save");

    expect(error.message).toBe("Test error");
    expect(error.operation).toBe("save");
    expect(error).toBeInstanceOf(StorageError);
  });

  it("should include context in error", () => {
    const error = new StorageError("Test error", "load", { id: "test-id" });

    expect(error.context).toEqual({ id: "test-id", operation: "load" });
  });

  it("should include cause in error", () => {
    const cause = new Error("Original error");
    const error = new StorageError("Test error", "save", undefined, cause);

    expect(error.cause).toBe(cause);
  });

  it("should have default severity as error", () => {
    const error = new StorageError("Test error", "save");

    expect(error.severity).toBe("error");
  });
});

describe("StorageQuotaExceededError", () => {
  it("should create error with quota details", () => {
    const error = new StorageQuotaExceededError("Quota exceeded", 1000, 500);

    expect(error.message).toBe("Quota exceeded");
    expect(error.requiredBytes).toBe(1000);
    expect(error.availableBytes).toBe(500);
    expect(error.operation).toBe("quota");
  });

  it("should include quota info in context", () => {
    const error = new StorageQuotaExceededError("Quota exceeded", 1000, 500, { path: "/data" });

    expect(error.context).toEqual({
      path: "/data",
      operation: "quota",
      requiredBytes: 1000,
      availableBytes: 500,
    });
  });
});

describe("EntityNotFoundError", () => {
  it("should create error with entity details", () => {
    const error = new EntityNotFoundError("Entity not found", "entity-123", "checkpoint");

    expect(error.message).toBe("Entity not found");
    expect(error.entityId).toBe("entity-123");
    expect(error.entityType).toBe("checkpoint");
    expect(error.operation).toBe("load");
  });

  it("should include entity info in context", () => {
    const error = new EntityNotFoundError("Entity not found", "entity-123", "checkpoint", {
      userId: "user-1",
    });

    expect(error.context).toEqual({
      userId: "user-1",
      operation: "load",
      entityId: "entity-123",
      entityType: "checkpoint",
    });
  });
});

describe("StorageInitializationError", () => {
  it("should create error with message", () => {
    const error = new StorageInitializationError("Failed to initialize");

    expect(error.message).toBe("Failed to initialize");
    expect(error.operation).toBe("initialize");
  });

  it("should include cause in error", () => {
    const cause = new Error("Connection failed");
    const error = new StorageInitializationError("Failed to initialize", cause);

    expect(error.cause).toBe(cause);
  });

  it("should include context in error", () => {
    const error = new StorageInitializationError("Failed to initialize", undefined, {
      dbPath: "/path/to/db",
    });

    expect(error.context).toEqual({
      dbPath: "/path/to/db",
      operation: "initialize",
    });
  });
});

describe("SerializationError", () => {
  it("should create error with entity id", () => {
    const error = new SerializationError("Failed to serialize", "entity-123");

    expect(error.message).toBe("Failed to serialize");
    expect(error.entityId).toBe("entity-123");
    expect(error.operation).toBe("serialize");
  });

  it("should include cause in error", () => {
    const cause = new Error("JSON.stringify failed");
    const error = new SerializationError("Failed to serialize", "entity-123", cause);

    expect(error.cause).toBe(cause);
  });

  it("should include context in error", () => {
    const error = new SerializationError("Failed to serialize", "entity-123", undefined, {
      format: "json",
    });

    expect(error.context).toEqual({
      format: "json",
      operation: "serialize",
      entityId: "entity-123",
    });
  });
});
