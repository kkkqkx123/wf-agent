/**
 * MessageStream Unit Tests
 * Tests the event-driven streaming response processing
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MessageStream } from "../message-stream.js";
import { ExecutionError } from "@wf-agent/types";
import type { LLMMessage, LLMResult } from "@wf-agent/types";

function createTestMessage(overrides: Partial<LLMMessage> = {}): LLMMessage {
  return {
    role: "assistant",
    content: "",
    ...overrides,
  };
}

function createTestResult(overrides: Partial<LLMResult> = {}): LLMResult {
  return {
    id: "test-id",
    model: "test-model",
    content: "test content",
    message: createTestMessage({ content: "test content" }),
    finishReason: "stop",
    duration: 100,
    ...overrides,
  };
}

describe("MessageStream", () => {
  let stream: MessageStream;

  beforeEach(() => {
    stream = new MessageStream();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("constructor", () => {
    it("should initialize with default values", () => {
      expect(stream.isEnded()).toBe(false);
      expect(stream.isErrored()).toBe(false);
      expect(stream.isAborted()).toBe(false);
      expect(stream.getReceivedMessages()).toEqual([]);
      expect(stream.getRequestId()).toBeNull();
      expect(stream.getResponse()).toBeNull();
      expect(stream.getController()).toBeInstanceOf(AbortController);
    });

    it("should initialize dead loop detector by default", () => {
      const s = new MessageStream();
      expect((s as any).deadLoopDetector).toBeDefined();
    });

    it("should disable dead loop detector when configured", () => {
      const s = new MessageStream({ enableDeadLoopDetection: false });
      expect((s as any).deadLoopDetector).toBeUndefined();
    });
  });

  describe("event listener management", () => {
    it("should add event listener with on()", () => {
      const listener = vi.fn();
      stream.on("text", listener);
      const listeners = (stream as any).listeners.get("text");
      expect(listeners).toHaveLength(1);
      expect(listeners[0].listener).toBe(listener);
      expect(listeners[0].once).toBe(false);
    });

    it("should support chaining on()", () => {
      const result = stream.on("text", vi.fn());
      expect(result).toBe(stream);
    });

    it("should add one-time listener with once()", () => {
      const listener = vi.fn();
      stream.once("text", listener);
      const listeners = (stream as any).listeners.get("text");
      expect(listeners).toHaveLength(1);
      expect(listeners[0].once).toBe(true);
    });

    it("should remove listener with off()", () => {
      const listener = vi.fn();
      stream.on("text", listener);
      stream.off("text", listener);
      const listeners = (stream as any).listeners.get("text");
      expect(listeners).toHaveLength(0);
    });

    it("should return this from off() for chaining", () => {
      const result = stream.off("text", vi.fn());
      expect(result).toBe(stream);
    });

    it("should do nothing when removing non-existent listener", () => {
      stream.on("text", vi.fn());
      const result = stream.off("text", vi.fn());
      expect(result).toBe(stream);
    });

    it("should do nothing when removing listener from non-existent event", () => {
      const result = stream.off("nonexistent" as any, vi.fn());
      expect(result).toBe(stream);
    });
  });

  describe("pushText", () => {
    it("should emit text event with delta and snapshot", () => {
      const listener = vi.fn();
      stream.on("text", listener);

      stream.pushText("Hello");

      expect(listener).toHaveBeenCalledWith("Hello", "Hello");
    });

    it("should accumulate text across multiple pushes", () => {
      const listener = vi.fn();
      stream.on("text", listener);

      stream.pushText("Hello ");
      stream.pushText("World");

      expect(listener).toHaveBeenLastCalledWith("World", "Hello World");
    });

    it("should not emit text after end", () => {
      const listener = vi.fn();
      stream.on("text", listener);
      stream.end();

      stream.pushText("Hello");

      expect(listener).not.toHaveBeenCalled();
    });

    it("should not emit text after abort", () => {
      const listener = vi.fn();
      stream.on("text", listener);
      stream.abort();

      stream.pushText("Hello");

      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("pushReasoning", () => {
    it("should emit reasoningText event with delta and snapshot", () => {
      const listener = vi.fn();
      stream.on("reasoningText", listener);

      stream.pushReasoning("thinking...");

      expect(listener).toHaveBeenCalledWith("thinking...", "thinking...");
    });

    it("should accumulate reasoning content across multiple pushes", () => {
      const listener = vi.fn();
      stream.on("reasoningText", listener);

      stream.pushReasoning("step 1. ");
      stream.pushReasoning("step 2.");

      expect(listener).toHaveBeenLastCalledWith("step 2.", "step 1. step 2.");
    });

    it("should not emit reasoningText after end", () => {
      const listener = vi.fn();
      stream.on("reasoningText", listener);
      stream.end();

      stream.pushReasoning("thinking...");

      expect(listener).not.toHaveBeenCalled();
    });

    it("should not emit after abort", () => {
      const listener = vi.fn();
      stream.on("reasoningText", listener);
      stream.abort();

      stream.pushReasoning("thinking...");

      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("end", () => {
    it("should emit end event and mark as ended", () => {
      const listener = vi.fn();
      stream.on("end", listener);

      stream.end();

      expect(listener).toHaveBeenCalled();
      expect(stream.isEnded()).toBe(true);
    });

    it("should emit finalMessage if messages exist", () => {
      const listener = vi.fn();
      stream.on("finalMessage", listener);

      // Simulate receiving a message
      (stream as any).receivedMessages.push(createTestMessage({ content: "Hello" }));
      stream.end();

      expect(listener).toHaveBeenCalled();
      expect(listener).toHaveBeenCalledWith(expect.objectContaining({ content: "Hello" }));
    });

    it("should not emit finalMessage if no messages exist", () => {
      const listener = vi.fn();
      stream.on("finalMessage", listener);

      stream.end();

      expect(listener).not.toHaveBeenCalled();
    });

    it("should be idempotent", () => {
      const listener = vi.fn();
      stream.on("end", listener);

      stream.end();
      stream.end();

      expect(listener).toHaveBeenCalledTimes(1);
    });
  });

  describe("abort", () => {
    it("should emit abort event and mark as aborted", () => {
      const listener = vi.fn();
      stream.on("abort", listener);

      stream.abort();

      expect(listener).toHaveBeenCalledWith("Stream aborted by user");
      expect(stream.isAborted()).toBe(true);
    });

    it("should also emit end after abort", () => {
      const endListener = vi.fn();
      stream.on("end", endListener);

      stream.abort();

      expect(endListener).toHaveBeenCalled();
    });

    it("should be idempotent", () => {
      const listener = vi.fn();
      stream.on("abort", listener);

      stream.abort();
      stream.abort();

      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("should not abort if already ended", () => {
      const listener = vi.fn();
      stream.on("abort", listener);
      stream.end();

      stream.abort();

      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("error flow", () => {
    it("should emit error event", () => {
      const listener = vi.fn();
      stream.on("error", listener);
      const testError = new Error("Test error");

      // Trigger error via private emit
      (stream as any).emit("error", { type: "error", error: testError });

      expect(listener).toHaveBeenCalledWith(testError);
      expect(stream.isErrored()).toBe(true);
    });

    it("should also emit end after error", () => {
      const endListener = vi.fn();
      stream.on("end", endListener);

      (stream as any).emit("error", { type: "error", error: new Error("Test") });

      expect(endListener).toHaveBeenCalled();
    });
  });

  describe("done / finalMessage / finalText / getFinalResult", () => {
    it("done() should resolve when stream ends", async () => {
      const donePromise = stream.done();
      stream.end();
      await expect(donePromise).resolves.toBeUndefined();
    });

    it("finalMessage() should resolve to the last received message", async () => {
      const msg1 = createTestMessage({ content: "First" });
      const msg2 = createTestMessage({ content: "Second" });
      (stream as any).receivedMessages.push(msg1, msg2);

      // Need to end the stream to resolve done()
      // finalMessage waits for done(), so simulate end
      stream.end();

      const result = await stream.finalMessage();
      expect(result.content).toBe("Second");
    });

    it("finalMessage() should throw if no messages received", async () => {
      stream.end();
      await expect(stream.finalMessage()).rejects.toThrow(ExecutionError);
    });

    it("finalText() should extract text from string content", async () => {
      (stream as any).receivedMessages.push(createTestMessage({ content: "Hello World" }));
      stream.end();

      const text = await stream.finalText();
      expect(text).toBe("Hello World");
    });

    it("finalText() should extract text from array content", async () => {
      (stream as any).receivedMessages.push(
        createTestMessage({
          content: [
            { type: "text", text: "Hello " },
            { type: "text", text: "World" },
          ] as any,
        }),
      );
      stream.end();

      const text = await stream.finalText();
      expect(text).toBe("Hello World");
    });

    it("finalText() should return empty string for empty array", async () => {
      (stream as any).receivedMessages.push(createTestMessage({ content: [] }));
      stream.end();

      const text = await stream.finalText();
      expect(text).toBe("");
    });

    it("getFinalResult() should return the set result", async () => {
      const result = createTestResult();
      stream.setFinalResult(result);
      stream.end();

      const final = await stream.getFinalResult();
      expect(final).toBe(result);
    });

    it("getFinalResult() should throw if no result set", async () => {
      stream.end();
      await expect(stream.getFinalResult()).rejects.toThrow(ExecutionError);
    });
  });

  describe("emitted()", () => {
    it("should resolve when the event is emitted", async () => {
      const emittedPromise = stream.emitted("text");

      stream.pushText("Hello");

      const result = await emittedPromise;
      expect(result).toBe("Hello");
    });

    it("should reject on error event", async () => {
      const emittedPromise = stream.emitted("end");
      const testError = new Error("Stream error");

      (stream as any).emit("error", { type: "error", error: testError });

      await expect(emittedPromise).rejects.toThrow("Stream error");
    });
  });

  describe("setAbortSignal", () => {
    it("should abort immediately if signal is already aborted", () => {
      const controller = new AbortController();
      controller.abort();

      stream.setAbortSignal(controller.signal);

      expect(stream.isAborted()).toBe(true);
    });

    it("should abort when external signal aborts", () => {
      const controller = new AbortController();

      stream.setAbortSignal(controller.signal);
      controller.abort();

      expect(stream.isAborted()).toBe(true);
    });

    it("should ignore null/undefined signal", () => {
      stream.setAbortSignal(null as any);
      stream.setAbortSignal(undefined as any);
      expect(stream.isAborted()).toBe(false);
    });

    it("should clean up listener on stream end", () => {
      const controller = new AbortController();
      const removeSpy = vi.spyOn(controller.signal, "removeEventListener");

      stream.setAbortSignal(controller.signal);
      stream.end();

      expect(removeSpy).toHaveBeenCalled();
    });
  });

  describe("accumulateMessage", () => {
    it("should handle message_start event", () => {
      const connectListener = vi.fn();
      stream.on("connect", connectListener);

      const result = stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "" } },
      });

      expect(connectListener).toHaveBeenCalled();
      expect(result).toEqual(
        expect.objectContaining({ role: "assistant", content: expect.any(String) }),
      );
      expect((stream as any).currentMessageSnapshot).toBeTruthy();
    });

    it("should throw on duplicate message_start", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "" } },
      });

      expect(() =>
        stream.accumulateMessage({
          type: "message_start",
          data: { message: { role: "assistant", content: "" } },
        }),
      ).toThrow(ExecutionError);
    });

    it("should handle message_delta with stop_reason", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "" } },
      });

      const result = stream.accumulateMessage({
        type: "message_delta",
        data: { delta: { stop_reason: "end_turn" } },
      });

      expect((result as any).stop_reason).toBe("end_turn");
    });

    it("should handle message_delta with usage merge", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "" } },
      });

      stream.accumulateMessage({
        type: "message_delta",
        data: { delta: {}, usage: { input_tokens: 10, output_tokens: 20 } },
      });

      stream.accumulateMessage({
        type: "message_delta",
        data: { delta: {}, usage: { output_tokens: 25 } },
      });

      const snapshot = (stream as any).currentMessageSnapshot;
      expect(snapshot.usage).toEqual({ input_tokens: 10, output_tokens: 25 });
    });

    it("should throw message_delta without active message", () => {
      expect(() =>
        stream.accumulateMessage({
          type: "message_delta",
          data: { delta: {} },
        }),
      ).toThrow(ExecutionError);
    });

    it("should handle content_block_start with text block", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "" } },
      });

      const result = stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "text" } },
      });

      expect(Array.isArray((result as any).content)).toBe(true);
      expect((result as any).content).toHaveLength(1);
      expect((result as any).content[0].type).toBe("text");
    });

    it("should throw content_block_start without active message", () => {
      expect(() =>
        stream.accumulateMessage({
          type: "content_block_start",
          data: { content_block: { type: "text" } },
        }),
      ).toThrow(ExecutionError);
    });

    it("should handle content_block_delta text_delta", () => {
      const textListener = vi.fn();
      stream.on("text", textListener);

      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "text" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: { index: 0, delta: { type: "text_delta", text: "Hello" } },
      });

      expect((stream as any).currentTextSnapshot).toBe("Hello");
      expect(textListener).toHaveBeenCalledWith("Hello", "Hello");
    });

    it("should handle content_block_delta input_json_delta", () => {
      const inputJsonListener = vi.fn();
      stream.on("inputJson", inputJsonListener);

      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "tool_use", id: "tool1", name: "test_tool" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: { index: 0, delta: { type: "input_json_delta", partial_json: '{"key": "value"}' } },
      });

      expect(inputJsonListener).toHaveBeenCalled();
      const callArg = inputJsonListener.mock.calls[0]!;
      expect(callArg[0]).toBe('{"key": "value"}');
    });

    it("should handle content_block_stop cleaning up __json_buf", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "tool_use", id: "tool1", name: "test_tool" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: { index: 0, delta: { type: "input_json_delta", partial_json: "{}" } },
      });

      stream.accumulateMessage({
        type: "content_block_stop",
        data: { index: 0 },
      });

      const snapshot = (stream as any).currentMessageSnapshot;
      expect(snapshot.content[0].__json_buf).toBeUndefined();
    });

    it("should handle content_block_delta thinking_delta", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "thinking" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: { index: 0, delta: { type: "thinking_delta", thinking: "step 1" } },
      });

      const snapshot = (stream as any).currentMessageSnapshot;
      expect(snapshot.content[0].thinking).toBe("step 1");
    });

    it("should handle content_block_delta signature_delta", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "thinking" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: { index: 0, delta: { type: "signature_delta", signature: "sig123" } },
      });

      const snapshot = (stream as any).currentMessageSnapshot;
      expect(snapshot.content[0].signature).toBe("sig123");
    });

    it("should handle content_block_delta citations_delta", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: [] } },
      });

      stream.accumulateMessage({
        type: "content_block_start",
        data: { content_block: { type: "text" } },
      });

      stream.accumulateMessage({
        type: "content_block_delta",
        data: {
          index: 0,
          delta: { type: "citations_delta", citation: { cited_text: "source" } },
        },
      });

      const snapshot = (stream as any).currentMessageSnapshot;
      expect(snapshot.content[0].citations).toBeDefined();
      expect(snapshot.content[0].citations).toHaveLength(1);
    });

    it("should handle message_stop event", () => {
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "Hello" } },
      });

      const msgListener = vi.fn();
      stream.on("message", msgListener);

      const result = stream.accumulateMessage({
        type: "message_stop",
        data: {},
      });

      expect(msgListener).toHaveBeenCalled();
      expect(result).toEqual(expect.objectContaining({ role: "assistant", content: "Hello" }));
      expect((stream as any).currentMessageSnapshot).toBeNull();
      expect(stream.getReceivedMessages()).toHaveLength(1);
    });

    it("should handle unknown event types gracefully", () => {
      const result = stream.accumulateMessage({
        type: "unknown_event",
        data: {},
      });

      expect(result).toBeNull();
    });
  });

  describe("AsyncIterator", () => {
    it("should yield stream events via async iteration", async () => {
      const events: Array<{ type: string }> = [];

      // Start listening via async iterator
      const iterator = stream[Symbol.asyncIterator]();

      // Push events
      stream.accumulateMessage({
        type: "message_start",
        data: { message: { role: "assistant", content: "Hello" } },
      });

      stream.accumulateMessage({
        type: "message_stop",
        data: {},
      });

      stream.end();

      for await (const event of { [Symbol.asyncIterator]: () => iterator }) {
        events.push({ type: event.type });
        if (event.type === "end") break;
      }

      expect(events.length).toBeGreaterThan(0);
    });

    it("should support return() to abort", async () => {
      const iterator = stream[Symbol.asyncIterator]();
      const result = await iterator.return!();
      expect(result.done).toBe(true);
      expect(stream.isAborted()).toBe(true);
    });
  });

  describe("tee", () => {
    it("should split stream into two", () => {
      const [left, right] = stream.tee();
      expect(left).toBeInstanceOf(MessageStream);
      expect(right).toBeInstanceOf(MessageStream);
    });

    it("should share the same controller between tee'd streams", () => {
      const [left, right] = stream.tee();
      const controller = stream.getController();
      expect((left as any).controller).toBe(controller);
      expect((right as any).controller).toBe(controller);
    });
  });

  describe("setRequestId / getRequestId", () => {
    it("should set and get request ID", () => {
      stream.setRequestId("req-123");
      expect(stream.getRequestId()).toBe("req-123");
    });
  });

  describe("setResponse / getResponse", () => {
    it("should set and get response", () => {
      const response = new Response("test", { status: 200 });
      stream.setResponse(response);
      expect(stream.getResponse()).toBe(response);
    });
  });

  describe("setFinalResult", () => {
    it("should store the final result", () => {
      const result = createTestResult();
      stream.setFinalResult(result);
      expect((stream as any).finalResultValue).toBe(result);
    });
  });

  describe("getReceivedMessages", () => {
    it("should return a copy of received messages", () => {
      const msg = createTestMessage({ content: "Hello" });
      (stream as any).receivedMessages.push(msg);
      const messages = stream.getReceivedMessages();
      expect(messages).toEqual([msg]);
      expect(messages).not.toBe((stream as any).receivedMessages);
    });
  });

  describe("cleanup", () => {
    it("should clear listeners and abort on cleanup", () => {
      stream.on("text", vi.fn());
      stream.cleanup();

      expect((stream as any).listeners.size).toBe(0);
      expect((stream as any).pushQueue).toEqual([]);
      expect((stream as any).readQueue).toEqual([]);
      expect(stream.isAborted()).toBe(true);
    });

    it("should not double-abort on cleanup", () => {
      stream.cleanup();
      // Second cleanup should not throw
      stream.cleanup();
    });
  });

  describe("dead loop detection", () => {
    it("should not be enabled when configured off", () => {
      const s = new MessageStream({ enableDeadLoopDetection: false });
      expect((s as any).deadLoopDetector).toBeUndefined();
    });

    it("should call onDeadLoopDetected callback when dead loop is detected", () => {
      const onDeadLoopDetected = vi.fn();
      const s = new MessageStream({
        enableDeadLoopDetection: true,
        deadLoopConfig: {
          checkpoints: [5],
          minRepeatUnitLength: 1,
          minRepeatCount: 2,
        },
        onDeadLoopDetected,
      });

      // Mock the detector to always detect
      const mockDetector = {
        detect: vi.fn().mockReturnValue({ detected: true, type: "repeat", details: {} }),
        reset: vi.fn(),
      };
      (s as any).deadLoopDetector = mockDetector;

      s.pushReasoning("aaaaaa");

      expect(onDeadLoopDetected).toHaveBeenCalledWith({
        detected: true,
        type: "repeat",
        details: {},
      });
      expect(s.isAborted()).toBe(true);
    });

    it("should handle dead loop detector errors gracefully", () => {
      const mockDetector = {
        detect: vi.fn().mockImplementation(() => {
          throw new Error("Detector error");
        }),
        reset: vi.fn(),
      };
      const s = new MessageStream();
      (s as any).deadLoopDetector = mockDetector;

      // Should not throw
      expect(() => s.pushReasoning("test")).not.toThrow();
    });

    it("should reset dead loop detector", () => {
      const resetFn = vi.fn();
      const mockDetector = { detect: vi.fn(), reset: resetFn };
      const s = new MessageStream();
      (s as any).deadLoopDetector = mockDetector;

      s.resetDeadLoopDetector();

      expect(resetFn).toHaveBeenCalled();
      expect((s as any).reasoningMessage).toBe("");
    });
  });

  describe("one-time listeners", () => {
    it("should only fire once", () => {
      const listener = vi.fn();
      stream.once("text", listener);

      stream.pushText("First");
      stream.pushText("Second");

      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("should not remove other listeners", () => {
      const permanentListener = vi.fn();
      const onceListener = vi.fn();
      stream.on("text", permanentListener);
      stream.once("text", onceListener);

      stream.pushText("First");
      stream.pushText("Second");

      expect(permanentListener).toHaveBeenCalledTimes(2);
      expect(onceListener).toHaveBeenCalledTimes(1);
    });
  });

  describe("listener error isolation", () => {
    it("should not let a throwing listener affect others", () => {
      const throwingListener = vi.fn().mockImplementation(() => {
        throw new Error("Listener error");
      });
      const normalListener = vi.fn();

      stream.on("text", throwingListener);
      stream.on("text", normalListener);

      expect(() => stream.pushText("test")).not.toThrow();
      expect(normalListener).toHaveBeenCalled();
    });
  });

  describe("event emission on ended stream", () => {
    it("should not emit events after end", () => {
      const listener = vi.fn();
      stream.on("text", listener);
      stream.end();
      stream.pushText("test");
      expect(listener).not.toHaveBeenCalled();
    });
  });
});
