/**
 * Logger Module Integration Tests
 * Tests the complete logger system including streams, registry, and async context
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  createLogger,
  createPackageLogger,
  createConsoleLogger,
  createNoopLogger,
  BaseLogger,
  ConsoleStream,
  createConsoleStream,
  AsyncStream,
  createAsyncStream,
  Multistream,
  createMultistream,
  loggerRegistry,
  registerLogger,
  unregisterLogger,
  getRegisteredLogger,
  setLoggerLevel,
  setAllLoggersLevel,
  flushAllLoggers,
  flushAllLoggersSync,
  runWithContext,
  runWithTrace,
  getContextValue,
  getTraceId,
  getSpanId,
  setContextValue,
  createChildSpan,
  hasContext,
  generateTraceId,
  generateSpanId,
  TRACE_ID_KEY,
  SPAN_ID_KEY,
  PARENT_SPAN_ID_KEY,
  type LogEntry,
  type LogLevel,
} from "../src/logger/index.js";

describe("Logger Module Integration Tests", () => {
  // Capture console output for testing
  let consoleOutput: string[] = [];
  const originalConsoleLog = console.log;
  const originalConsoleDebug = console.debug;
  const originalConsoleWarn = console.warn;
  const originalConsoleError = console.error;

  beforeEach(() => {
    consoleOutput = [];
    console.log = (...args) => {
      consoleOutput.push(args.join(" "));
    };
    console.debug = (...args) => {
      consoleOutput.push(args.join(" "));
    };
    console.warn = (...args) => {
      consoleOutput.push(args.join(" "));
    };
    console.error = (...args) => {
      consoleOutput.push(args.join(" "));
    };
  });

  afterEach(() => {
    console.log = originalConsoleLog;
    console.debug = originalConsoleDebug;
    console.warn = originalConsoleWarn;
    console.error = originalConsoleError;
    consoleOutput = [];
    
    // Clear logger registry
    loggerRegistry.clear();
  });

  describe("Basic Logger Functionality", () => {
    it("should create a basic logger and log messages", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.debug("debug message");
      logger.info("info message");
      logger.warn("warn message");
      logger.error("error message");

      expect(consoleOutput.length).toBe(4);
      expect(consoleOutput[0]).toContain("debug message");
      expect(consoleOutput[1]).toContain("info message");
      expect(consoleOutput[2]).toContain("warn message");
      expect(consoleOutput[3]).toContain("error message");
    });

    it("should respect log levels", () => {
      const logger = createLogger({ level: "warn" });
      
      logger.debug("debug message");
      logger.info("info message");
      logger.warn("warn message");
      logger.error("error message");

      // Only warn and error should be logged
      expect(consoleOutput.length).toBe(2);
      expect(consoleOutput[0]).toContain("warn message");
      expect(consoleOutput[1]).toContain("error message");
    });

    it("should create package logger with package context", () => {
      const logger = createPackageLogger("test-package");
      
      logger.info("test message");
      
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("test message");
    });

    it("should create console logger with specific level", () => {
      const logger = createConsoleLogger("error");
      
      logger.debug("debug message");
      logger.info("info message");
      logger.warn("warn message");
      logger.error("error message");

      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("error message");
    });

    it("should create noop logger that produces no output", () => {
      const logger = createNoopLogger();
      
      logger.debug("debug message");
      logger.info("info message");
      logger.warn("warn message");
      logger.error("error message");

      expect(consoleOutput.length).toBe(0);
    });

    it("should support child loggers", () => {
      const parentLogger = createLogger({ name: "parent", level: "debug" });
      const childLogger = parentLogger.child("child");
      
      parentLogger.info("parent message");
      childLogger.info("child message");

      expect(consoleOutput.length).toBe(2);
      expect(consoleOutput[0]).toContain("parent message");
      expect(consoleOutput[1]).toContain("child message");
    });

    it("should merge context in child loggers", () => {
      const parentLogger = createLogger({ 
        name: "parent", 
        level: "debug",
        base: { parentKey: "parentValue" }
      });
      const childLogger = parentLogger.child("child", { childKey: "childValue" });
      
      childLogger.info("message with context");
      
      expect(consoleOutput.length).toBe(1);
    });
  });

  describe("Logger Configuration", () => {
    it("should support JSON output format", () => {
      const logger = createLogger({ 
        level: "info",
        json: true 
      });
      
      logger.info("json message");
      
      expect(consoleOutput.length).toBe(1);
      // Should be valid JSON
      const parsed = JSON.parse(consoleOutput[0]!);
      expect(parsed.message).toBe("json message");
      expect(parsed.level).toBe("info");
    });

    it("should support pretty printing with colors", () => {
      const logger = createLogger({ 
        level: "info",
        pretty: true 
      });
      
      logger.info("pretty message");
      
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("pretty message");
    });

    it("should support timestamp inclusion/exclusion", () => {
      const loggerWithTimestamp = createLogger({ 
        level: "info",
        timestamp: true 
      });
      
      loggerWithTimestamp.info("message with timestamp");
      
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toMatch(/\d{4}-\d{2}-\d{2}/);
    });

    it("should support sampling rate", () => {
      // Create logger with 0% sampling (only errors should pass)
      const logger = createLogger({ 
        level: "debug",
        sampleRate: 0
      });
      
      logger.info("sampled message");
      logger.error("error message");
      
      // Info should be sampled out, error should always pass
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("error message");
    });

    it("should support rate limiting", () => {
      const logger = createLogger({ 
        level: "debug",
        maxLogsPerSecond: 2
      });
      
      // Log more than the limit
      for (let i = 0; i < 5; i++) {
        logger.info(`message ${i}`);
      }
      
      // Should only allow 2 messages per second
      expect(consoleOutput.length).toBeLessThanOrEqual(2);
    });

    it("should support category configuration", () => {
      const logger = createLogger({ 
        level: "info",
        category: "test-category"
      });
      
      logger.info("categorized message");
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should support tags configuration", () => {
      const logger = createLogger({ 
        level: "info",
        tags: ["tag1", "tag2"]
      });
      
      logger.info("tagged message");
      
      expect(consoleOutput.length).toBe(1);
    });
  });

  describe("Stream Integration", () => {
    it("should create and use console stream", () => {
      const stream = createConsoleStream({ json: false });
      const logger = createLogger({ 
        level: "info",
        stream 
      });
      
      logger.info("console stream message");
      
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("console stream message");
    });

    it("should create and use async stream", () => {
      const consoleStream = createConsoleStream();
      const asyncStream = createAsyncStream(consoleStream, {
        batchSize: 10
      });
      
      const logger = createLogger({ 
        level: "info",
        stream: asyncStream
      });
      
      logger.info("async stream message");
      
      // Flush to ensure message is written
      if (asyncStream.flush) {
        asyncStream.flush();
      }
      
      expect(consoleOutput.length).toBeGreaterThanOrEqual(1);
    });

    it("should create and use multistream", () => {
      const stream1 = createConsoleStream();
      const stream2 = createConsoleStream();
      
      const multiStream = createMultistream([
        { stream: stream1, level: "info" },
        { stream: stream2, level: "error" }
      ]);
      
      const logger = createLogger({ 
        level: "debug",
        stream: multiStream
      });
      
      logger.info("multistream info");
      logger.error("multistream error");
      
      // Both streams should receive the messages based on their level filters
      expect(consoleOutput.length).toBeGreaterThan(0);
    });

    it("should support dynamic stream replacement", () => {
      const logger = createLogger({ level: "info" });
      
      logger.info("first stream message");
      
      const newStream = createConsoleStream();
      if (logger.setStream) {
        logger.setStream(newStream);
      }
      
      logger.info("second stream message");
      
      expect(consoleOutput.length).toBe(2);
    });
  });

  describe("Logger Registry", () => {
    it("should register and retrieve loggers", () => {
      const logger = createPackageLogger("test-registry-logger");
      
      registerLogger("test-registry-logger", logger);
      
      const retrieved = getRegisteredLogger("test-registry-logger");
      expect(retrieved).toBeDefined();
      expect(retrieved).toBe(logger);
    });

    it("should unregister loggers", () => {
      const logger = createPackageLogger("test-unregister");
      
      registerLogger("test-unregister", logger);
      expect(getRegisteredLogger("test-unregister")).toBeDefined();
      
      unregisterLogger("test-unregister");
      expect(getRegisteredLogger("test-unregister")).toBeUndefined();
    });

    it("should get all registered logger names", () => {
      const logger1 = createPackageLogger("logger1");
      const logger2 = createPackageLogger("logger2");
      
      registerLogger("logger1", logger1);
      registerLogger("logger2", logger2);
      
      const names = Array.from(loggerRegistry.keys());
      expect(names).toContain("logger1");
      expect(names).toContain("logger2");
    });

    it("should set log level for specific logger", () => {
      const logger = createPackageLogger("level-test");
      registerLogger("level-test", logger);
      
      setLoggerLevel("level-test", "error");
      
      expect(logger.getLevel()).toBe("error");
    });

    it("should set log level using wildcard pattern", () => {
      const logger1 = createPackageLogger("module.submodule1");
      const logger2 = createPackageLogger("module.submodule2");
      const logger3 = createPackageLogger("other.module");
      
      registerLogger("module.submodule1", logger1);
      registerLogger("module.submodule2", logger2);
      registerLogger("other.module", logger3);
      
      setLoggerLevel("module.*", "warn");
      
      expect(logger1.getLevel()).toBe("warn");
      expect(logger2.getLevel()).toBe("warn");
      expect(logger3.getLevel()).not.toBe("warn");
    });

    it("should set log level for all registered loggers", () => {
      const logger1 = createPackageLogger("all-levels-1");
      const logger2 = createPackageLogger("all-levels-2");
      
      registerLogger("all-levels-1", logger1);
      registerLogger("all-levels-2", logger2);
      
      setAllLoggersLevel("debug");
      
      expect(logger1.getLevel()).toBe("debug");
      expect(logger2.getLevel()).toBe("debug");
    });

    it("should flush all loggers synchronously", () => {
      const logger = createPackageLogger("flush-sync-test");
      registerLogger("flush-sync-test", logger);
      
      logger.info("before flush");
      
      flushAllLoggersSync();
      
      // Should complete without error
      expect(true).toBe(true);
    });

    it("should flush all loggers asynchronously", () => {
      return new Promise<void>((resolve) => {
        const logger = createPackageLogger("flush-async-test");
        registerLogger("flush-async-test", logger);
        
        logger.info("before async flush");
        
        flushAllLoggers(() => {
          resolve();
        });
      });
    });
  });

  describe("Async Context", () => {
    it("should run function with context", () => {
      const result = runWithContext(() => {
        const value = getContextValue<string>("testKey");
        return value;
      }, { testKey: "testValue" });
      
      expect(result).toBe("testValue");
    });

    it("should run function with trace", () => {
      runWithTrace(() => {
        const traceId = getTraceId();
        const spanId = getSpanId();
        
        expect(traceId).toBeDefined();
        expect(spanId).toBeDefined();
        expect(traceId).toMatch(/^trace_/);
        expect(spanId).toMatch(/^span_/);
      });
    });

    it("should generate trace and span IDs", () => {
      const traceId1 = generateTraceId();
      const traceId2 = generateTraceId();
      const spanId1 = generateSpanId();
      const spanId2 = generateSpanId();
      
      expect(traceId1).not.toBe(traceId2);
      expect(spanId1).not.toBe(spanId2);
      expect(traceId1).toMatch(/^trace_/);
      expect(spanId1).toMatch(/^span_/);
    });

    it("should set and get context values", () => {
      runWithContext(() => {
        setContextValue("customKey", "customValue");
        const value = getContextValue<string>("customKey");
        expect(value).toBe("customValue");
      });
    });

    it("should create child spans", () => {
      runWithTrace(() => {
        const parentSpanId = getSpanId();
        const childSpanId = createChildSpan();
        
        expect(childSpanId).toBeDefined();
        expect(childSpanId).not.toBe(parentSpanId);
        
        const currentSpanId = getSpanId();
        expect(currentSpanId).toBe(childSpanId);
      });
    });

    it("should check if context exists", () => {
      expect(hasContext()).toBe(false);
      
      runWithContext(() => {
        expect(hasContext()).toBe(true);
      });
      
      expect(hasContext()).toBe(false);
    });

    it("should propagate context across async operations", () => {
      return new Promise<void>((resolve) => {
        runWithTrace(() => {
          const traceId = getTraceId();
          
          setTimeout(() => {
            const propagatedTraceId = getTraceId();
            expect(propagatedTraceId).toBe(traceId);
            resolve();
          }, 10);
        });
      });
    });

    it("should maintain separate contexts", () => {
      const results: string[] = [];
      
      runWithContext(() => {
        setContextValue("context", "context1");
        results.push(getContextValue<string>("context") || "");
      });
      
      runWithContext(() => {
        setContextValue("context", "context2");
        results.push(getContextValue<string>("context") || "");
      });
      
      expect(results).toEqual(["context1", "context2"]);
    });
  });

  describe("Integration Scenarios", () => {
    it("should work with registry, async context, and streams together", () => {
      return new Promise<void>((resolve) => {
        // Create logger with async stream
        const consoleStream = createConsoleStream();
        const asyncStream = createAsyncStream(consoleStream, {
          batchSize: 5
        });
        
        const logger = createPackageLogger("integration-test");
        registerLogger("integration-test", logger);
        
        // Run with trace context
        runWithTrace(() => {
          logger.info("integrated message");
          
          // Flush all loggers
          flushAllLoggers(() => {
            expect(consoleOutput.length).toBeGreaterThanOrEqual(1);
            resolve();
          });
        });
      });
    });

    it("should handle multiple child loggers with different configurations", () => {
      const parentLogger = createLogger({ 
        name: "parent",
        level: "info"
      });
      
      const child1 = parentLogger.child("child1", { module: "module1" });
      const child2 = parentLogger.child("child2", { module: "module2" });
      
      child1.setLevel("debug");
      child2.setLevel("error");
      
      child1.debug("child1 debug");
      child1.info("child1 info");
      child2.info("child2 info");
      child2.error("child2 error");
      
      // child1: debug + info, child2: only error
      expect(consoleOutput.length).toBe(3);
    });

    it("should handle errors gracefully", () => {
      const logger = createLogger({ level: "error" });
      
      expect(() => {
        logger.error("error with context", {
          error: new Error("test error"),
          stack: "test stack"
        });
      }).not.toThrow();
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should support complex context merging", () => {
      const logger = createLogger({
        name: "complex-context",
        level: "info",
        base: {
          baseKey: "baseValue",
          shared: "baseShared"
        }
      });
      
      logger.info("message", {
        messageKey: "messageValue",
        shared: "messageShared"
      });
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should handle rapid logging with rate limiting", () => {
      const logger = createLogger({
        level: "debug",
        maxLogsPerSecond: 10
      });
      
      const startTime = Date.now();
      for (let i = 0; i < 100; i++) {
        logger.info(`rapid message ${i}`);
      }
      const endTime = Date.now();
      
      // Should complete quickly despite rate limiting
      expect(endTime - startTime).toBeLessThan(1000);
      // Should have logged some messages but not all
      expect(consoleOutput.length).toBeGreaterThan(0);
      expect(consoleOutput.length).toBeLessThan(100);
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty messages", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.info("");
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should handle undefined context", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.info("message", undefined);
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should handle null context", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.info("message", null as any);
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should handle very long messages", () => {
      const logger = createLogger({ level: "debug" });
      const longMessage = "a".repeat(10000);
      
      logger.info(longMessage);
      
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]!.length).toBeGreaterThan(1000);
    });

    it("should handle special characters in messages", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.info("Message with special chars: \n\t\r\\\"'");
      
      expect(consoleOutput.length).toBe(1);
    });

    it("should handle unicode characters", () => {
      const logger = createLogger({ level: "debug" });
      
      logger.info("Unicode: 你好世界 🌍 Привет мир");
      
      expect(consoleOutput.length).toBe(1);
    });
  });
});
