import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { SseTransport } from "../sse-transport.js";
import { parseSSELine } from "../sse-utils.js";

describe("SseTransport", () => {
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
    it("should create with default timeout", () => {
      transport = new SseTransport();
      expect(transport).toBeDefined();
    });

    it("should create with custom baseUrl and headers", () => {
      transport = new SseTransport("https://api.example.com", { Authorization: "Bearer tok" });
      expect(transport).toBeDefined();
    });
  });

  describe("execute (non-streaming)", () => {
    it("should return response data", async () => {
      const mockHeaders = new Headers({ "content-type": "application/json", "x-request-id": "req-1" });
      const mockResponse = new Response(JSON.stringify({ result: "ok" }), {
        status: 200,
        headers: mockHeaders,
      });

      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);
      transport = new SseTransport("https://api.example.com");

      const result = await transport.execute("/test");
      expect(result.status).toBe(200);
      expect(result.requestId).toBe("req-1");
    });

    it("should throw on non-ok response", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response("Not Found", { status: 404 }));
      transport = new SseTransport();

      await expect(transport.execute("/test")).rejects.toThrow("HTTP 404");
    });

    it("should handle query parameters", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response("ok", { status: 200 }));
      transport = new SseTransport("https://api.example.com");

      await transport.execute("/search", { query: { q: "hello", page: 1 } });

      const calledUrl = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(calledUrl).toContain("q=hello");
      expect(calledUrl).toContain("page=1");
    });
  });

  describe("executeStream (SSE streaming)", () => {
    it("should yield parsed SSE messages", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"msg": "hello"}\n'));
          controller.enqueue(encoder.encode('data: {"msg": "world"}\n'));
          controller.close();
        },
      });

      const mockResponse = new Response(stream, {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);
      transport = new SseTransport();

      const results: unknown[] = [];
      for await (const data of transport.executeStream("/stream")) {
        results.push(data);
      }
      expect(results).toEqual([{ msg: "hello" }, { msg: "world" }]);
    });

    it("should throw on non-ok response", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response("Error", { status: 500 }));
      transport = new SseTransport();

      const generator = transport.executeStream("/stream");
      await expect(generator.next()).rejects.toThrow("HTTP 500");
    });

    it("should throw when response body is null", async () => {
      const mockResponse = new Response(null, { status: 200 });
      // Mock the body to be null
      Object.defineProperty(mockResponse, "body", { value: null });

      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);
      transport = new SseTransport();

      const generator = transport.executeStream("/stream");
      await expect(generator.next()).rejects.toThrow("Response body is null");
    });

    it("should abort on timeout", async () => {
      vi.useFakeTimers();

      // Create a response that never sends data (to trigger timeout)
      const stream = new ReadableStream<Uint8Array>({
        start() {
          // Never send data - stream hangs
        },
      });

      const mockResponse = new Response(stream, {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);
      transport = new SseTransport("", undefined, 5000); // 5s timeout

      const generator = transport.executeStream("/stream");

      // Advance time past the timeout
      const promise = generator.next();
      vi.advanceTimersByTime(6000);

      await expect(promise).rejects.toThrow();
      vi.useRealTimers();
    }, 3000);
  });

  describe("parseSSELine (internal)", () => {
    // We use the exported parseSSELine from sse-utils to validate internal parsing behavior
    it("should parse JSON SSE lines", () => {
      expect(parseSSELine('data: {"key":"val"}')).toEqual({ key: "val" });
    });

    it("should return null for comment lines", () => {
      expect(parseSSELine(": comment")).toBeNull();
    });

    it("should return null for empty lines", () => {
      expect(parseSSELine("")).toBeNull();
    });
  });
});
