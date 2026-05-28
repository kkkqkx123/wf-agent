import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { StreamableHttpTransport } from "../streamable-http.js";

describe("StreamableHttpTransport", () => {
  let transport: StreamableHttpTransport;
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
      transport = new StreamableHttpTransport({
        type: "streamable-http",
        url: "https://example.com/mcp",
        headers: { Authorization: "Bearer tok" },
      });
      expect(transport.type).toBe("streamable-http");
      expect(transport.isConnected).toBe(false);
    });
  });

  describe("start", () => {
    it("should connect successfully on HEAD ok", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });

      await transport.start();
      expect(transport.isConnected).toBe(true);
    });

    it("should throw on HEAD failure", async () => {
      globalThis.fetch = vi.fn().mockRejectedValue(new Error("Connection refused"));
      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });

      await expect(transport.start()).rejects.toThrow("Connection refused");
      expect(transport.isConnected).toBe(false);
    });

    it("should be idempotent when already connected", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });

      await transport.start();
      await transport.start(); // Second call should not call fetch again
      expect(globalThis.fetch).toHaveBeenCalledTimes(1);
    });
  });

  describe("send", () => {
    it("should send message and handle JSON response", async () => {
      globalThis.fetch = vi.fn()
        .mockResolvedValueOnce(new Response(null, { status: 200 })) // HEAD for start
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ result: "ok" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        );

      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });
      await transport.start();

      const onData = vi.fn();
      transport.setHandlers({ onData });
      await transport.send({ jsonrpc: "2.0", method: "ping" });

      expect(onData).toHaveBeenCalledWith({ result: "ok" });
    });

    it("should handle streaming SSE response", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"msg": "hello"}\n'));
          controller.enqueue(encoder.encode('data: {"msg": "world"}\n'));
          controller.close();
        },
      });

      globalThis.fetch = vi.fn()
        .mockResolvedValueOnce(new Response(null, { status: 200 })) // HEAD
        .mockResolvedValueOnce(
          new Response(stream, {
            status: 200,
            headers: { "content-type": "text/event-stream" },
          }),
        );

      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });
      await transport.start();

      const onData = vi.fn();
      transport.setHandlers({ onData });
      await transport.send({ jsonrpc: "2.0", method: "ping" });

      expect(onData).toHaveBeenCalledTimes(2);
      expect(onData).toHaveBeenNthCalledWith(1, { msg: "hello" });
      expect(onData).toHaveBeenNthCalledWith(2, { msg: "world" });
    });

    it("should throw if not connected", async () => {
      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });
      await expect(transport.send({})).rejects.toThrow("Transport not connected");
    });
  });

  describe("close", () => {
    it("should close and mark disconnected", async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
      transport = new StreamableHttpTransport({ type: "streamable-http", url: "https://example.com/mcp" });
      await transport.start();

      const onClose = vi.fn();
      transport.setHandlers({ onClose });
      await transport.close();

      expect(transport.isConnected).toBe(false);
      expect(onClose).toHaveBeenCalled();
    });
  });
});
