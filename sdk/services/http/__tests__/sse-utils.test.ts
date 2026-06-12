import { describe, it, expect } from "vitest";
import { parseSSELine, parseSSELines, streamSSE, readSSEStream } from "../sse-utils.js";

describe("SSE Utilities", () => {
  describe("parseSSELine", () => {
    it("should parse JSON data line", () => {
      const result = parseSSELine('data: {"key": "value"}');
      expect(result).toEqual({ key: "value" });
    });

    it("should parse string data line", () => {
      const result = parseSSELine("data: hello world");
      expect(result).toBe("hello world");
    });

    it("should return null for [DONE] marker", () => {
      const result = parseSSELine("data: [DONE]");
      expect(result).toBeNull();
    });

    it("should return null for empty data", () => {
      const result = parseSSELine("data: ");
      expect(result).toBeNull();
    });

    it("should return null for non-data line", () => {
      const result = parseSSELine("event: complete");
      expect(result).toBeNull();
    });

    it("should return null for empty line", () => {
      const result = parseSSELine("");
      expect(result).toBeNull();
    });

    it("should parse JSON with whitespace", () => {
      const result = parseSSELine('data:   {"a":1}');
      expect(result).toEqual({ a: 1 });
    });
  });

  describe("parseSSELines", () => {
    it("should parse multiple lines and filter nulls", () => {
      const lines = ['data: {"a": 1}', "event: ping", 'data: {"b": 2}', "", "data: [DONE]"];
      const result = parseSSELines(lines);
      expect(result).toEqual([{ a: 1 }, { b: 2 }]);
    });

    it("should return empty array for no data lines", () => {
      const result = parseSSELines(["event: ping", ": comment", ""]);
      expect(result).toEqual([]);
    });

    it("should handle mixed JSON and string data", () => {
      const lines = ["data: hello", 'data: {"key": "val"}'];
      const result = parseSSELines(lines);
      expect(result).toEqual(["hello", { key: "val" }]);
    });
  });

  describe("streamSSE", () => {
    it("should yield parsed SSE messages from a stream", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"msg": "hello"}\n'));
          controller.enqueue(encoder.encode('data: {"msg": "world"}\n'));
          controller.close();
        },
      });

      const results: unknown[] = [];
      for await (const data of streamSSE(stream)) {
        results.push(data);
      }
      expect(results).toEqual([{ msg: "hello" }, { msg: "world" }]);
    });

    it("should handle partial lines across chunks", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"msg": "hel'));
          controller.enqueue(encoder.encode('lo"}\n'));
          controller.close();
        },
      });

      const results: unknown[] = [];
      for await (const data of streamSSE(stream)) {
        results.push(data);
      }
      expect(results).toEqual([{ msg: "hello" }]);
    });

    it("should filter out [DONE] markers", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"msg": "keep"}\n'));
          controller.enqueue(encoder.encode("data: [DONE]\n"));
          controller.close();
        },
      });

      const results: unknown[] = [];
      for await (const data of streamSSE(stream)) {
        results.push(data);
      }
      expect(results).toEqual([{ msg: "keep" }]);
    });

    it("should handle empty stream", async () => {
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.close();
        },
      });

      const results: unknown[] = [];
      for await (const data of streamSSE(stream)) {
        results.push(data);
      }
      expect(results).toEqual([]);
    });

    it("should release reader lock in finally", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"x":1}\n'));
          controller.close();
        },
      });

      for await (const _data of streamSSE(stream)) {
        break; // early exit to test finally
      }
      // After the for-await break, the generator's finally should have released the lock
      const reader = stream.getReader();
      const { done } = await reader.read();
      reader.releaseLock();
      expect(done).toBe(true);
    });
  });

  describe("readSSEStream", () => {
    it("should call handler for each SSE message", async () => {
      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoder.encode('data: {"a": 1}\n'));
          controller.enqueue(encoder.encode('data: {"b": 2}\n'));
          controller.close();
        },
      });

      const received: unknown[] = [];
      await readSSEStream(stream, data => received.push(data));
      expect(received).toEqual([{ a: 1 }, { b: 2 }]);
    });
  });
});
