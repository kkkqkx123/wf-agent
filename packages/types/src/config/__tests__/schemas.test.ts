/**
 * Configuration Schema Tests
 */

import { describe, it, expect } from "vitest";
import {
  StorageConfigSchema,
  PresetsConfigSchema,
  OutputConfigSchema,
  CompressionConfigSchema,
} from "../schemas.js";

describe("Configuration Schemas", () => {
  describe("StorageConfigSchema", () => {
    it("should validate JSON storage config", () => {
      const config = {
        type: "json" as const,
        json: {
          baseDir: "./storage",
          enableFileLock: false,
        },
      };

      const result = StorageConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate SQLite storage config", () => {
      const config = {
        type: "sqlite" as const,
        sqlite: {
          dbPath: "./storage/app.db",
          enableWAL: true,
          enableLogging: false,
          readonly: false,
          fileMustExist: false,
          timeout: 5000,
        },
      };

      const result = StorageConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate memory storage config", () => {
      const config = {
        type: "memory" as const,
      };

      const result = StorageConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should reject invalid storage type", () => {
      const config = {
        type: "invalid",
      };

      const result = StorageConfigSchema.safeParse(config);
      expect(result.success).toBe(false);
    });
  });

  describe("PresetsConfigSchema", () => {
    it("should validate context compression preset", () => {
      const config = {
        contextCompression: {
          enabled: true,
          prompt: "Test prompt",
          timeout: 30000,
          maxTriggers: 10,
        },
      };

      const result = PresetsConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate predefined tools preset", () => {
      const config = {
        predefinedTools: {
          enabled: true,
          allowList: ["readFile", "writeFile"],
          blockList: ["bash"],
          config: {
            readFile: {
              workspaceDir: "/workspace",
              maxFileSize: 1048576,
            },
          },
        },
      };

      const result = PresetsConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate predefined prompts preset", () => {
      const config = {
        predefinedPrompts: {
          enabled: true,
        },
      };

      const result = PresetsConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate complete presets config", () => {
      const config = {
        contextCompression: {
          enabled: true,
        },
        predefinedTools: {
          enabled: true,
        },
        predefinedPrompts: {
          enabled: true,
        },
      };

      const result = PresetsConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });
  });

  describe("OutputConfigSchema", () => {
    it("should validate output config", () => {
      const config = {
        dir: "./outputs",
        logFilePattern: "app-{date}.log",
        enableLogTerminal: true,
        enableSDKLogs: true,
        sdkLogLevel: "info" as const,
      };

      const result = OutputConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should reject invalid SDK log level", () => {
      const config = {
        dir: "./outputs",
        logFilePattern: "app-{date}.log",
        enableLogTerminal: true,
        enableSDKLogs: true,
        sdkLogLevel: "invalid",
      };

      const result = OutputConfigSchema.safeParse(config);
      expect(result.success).toBe(false);
    });
  });

  describe("CompressionConfigSchema", () => {
    it("should validate gzip compression", () => {
      const config = {
        enabled: true,
        algorithm: "gzip" as const,
        threshold: 1024,
      };

      const result = CompressionConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should validate brotli compression", () => {
      const config = {
        enabled: true,
        algorithm: "brotli" as const,
        threshold: 2048,
      };

      const result = CompressionConfigSchema.safeParse(config);
      expect(result.success).toBe(true);
    });

    it("should reject zlib algorithm", () => {
      const config = {
        enabled: true,
        algorithm: "zlib" as any,
        threshold: 1024,
      };

      const result = CompressionConfigSchema.safeParse(config);
      expect(result.success).toBe(false);
    });

    it("should reject negative threshold", () => {
      const config = {
        enabled: true,
        algorithm: "gzip" as const,
        threshold: -100,
      };

      const result = CompressionConfigSchema.safeParse(config);
      expect(result.success).toBe(false);
    });
  });
});
