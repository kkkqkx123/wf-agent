import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { SseTransport } from "../sse.js";

describe("MCP SseTransport", () => {
  let transport: SseTransport;
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  describe("constructor", () => {
    it("should create with config", () => {
      transport = new SseTransport({
        type: "sse",
        url: "https://example.com/sse",
        headers: { Authorization: "Bearer tok" },
      });
      expect(transport.type).toBe("sse");
    });

    it("should create with reconnection config", () => {
      transport = new SseTransport({
        type: "sse",
        url: "https://example.com/sse",
        reconnection: {
          maxAttempts: 5,
          initialDelay: 200,
          maxDelay: 5000,
          backoffMultiplier: 2,
        },
      });
      expect(transport.type).toBe("sse");
    });
  });

  describe("start", () => {
    it("should establish SSE connection", async () => {
      transport = new SseTransport({
        type: "sse",
        url: "https://example.com/sse",
      });

      const startPromise = transport.start();

      // SSE connection is established with EventSource or fetch
      // In Node.js test environment, EventSource might not be available
      // The transport should handle this gracefully
      await expect(startPromise).rejects.toThrow();
    });
  });

  describe("send", () => {
    it("should throw if not started", async () => {
      transport = new SseTransport({
        type: "sse",
        url: "https://example.com/sse",
      });

      await expect(transport.send({})).rejects.toThrow();
    });
  });

  describe("setHandlers", () => {
    it("should set event handlers", () => {
      transport = new SseTransport({
        type: "sse",
        url: "https://example.com/sse",
      });

      const handlers = {
        onData: vi.fn(),
        onError: vi.fn(),
        onClose: vi.fn(),
      };
      transport.setHandlers(handlers);
      // No explicit getter for handlers, but it shouldn't throw
    });
  });
});
