/**
 * Message Flow
 * Provides event-driven, streaming response processing
 */

import { ExecutionError } from "@wf-agent/types";
import type { LLMMessage, LLMResult } from "@wf-agent/types";
import { partialParse } from "./lib/partial-json-parser.js";
import {
  MessageStreamEvent,
  MessageStreamEventType,
  MessageStreamConnectEvent,
  MessageStreamStreamEvent,
  MessageStreamTextEvent,
  MessageStreamInputJsonEvent,
  MessageStreamMessageEvent,
  MessageStreamFinalMessageEvent,
  MessageStreamErrorEvent,
  MessageStreamAbortEvent,
  MessageStreamEndEvent,
} from "./message-stream-events.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MessageStream" });

/**
 * Event listener
 */
type EventListener<T = unknown> = (data: T) => void;

/**
 * Flagged event listener
 */
interface FlaggedEventListener<T = unknown> {
  listener: EventListener<T>;
  once: boolean;
}

/**
 * Streaming Events (for internal use)
 */
interface InternalStreamEvent {
  type: string;
  data: unknown;
}

/**
 * Message flow
 */
export class MessageStream implements AsyncIterable<InternalStreamEvent> {
  private messages: LLMMessage[];
  private receivedMessages: LLMMessage[];
  private currentMessageSnapshot: LLMMessage | null;
  private currentTextSnapshot: string;
  private finalResultValue: LLMResult | null;
  private controller: AbortController;
  private listeners: Map<MessageStreamEventType, FlaggedEventListener<unknown>[]>;
  private ended: boolean;
  private errored: boolean;
  private aborted: boolean;
  private response: Response | null;
  private requestId: string | null;
  private endPromise: Promise<void>;
  private endPromiseResolve!: () => void;
  private endPromiseReject!: (error: Error) => void;
  private catchingPromiseCreated: boolean;
  private pushQueue: InternalStreamEvent[];
  private readQueue: Array<(event: InternalStreamEvent) => void>;

  constructor() {
    this.messages = [];
    this.receivedMessages = [];
    this.currentMessageSnapshot = null;
    this.currentTextSnapshot = "";
    this.finalResultValue = null;
    this.controller = new AbortController();
    this.listeners = new Map();
    this.ended = false;
    this.errored = false;
    this.aborted = false;
    this.response = null;
    this.requestId = null;
    this.catchingPromiseCreated = false;
    this.pushQueue = [];
    this.readQueue = [];

    // Create an end Promise
    this.endPromise = new Promise((resolve, reject) => {
      this.endPromiseResolve = resolve;
      this.endPromiseReject = reject;
    });

    // Avoid unhandled Promise rejections.
    this.endPromise.catch(() => {});
  }

  /**
   * Add an event listener
   * @param event  Event type
   * @param listener  Listener function
   * @returns this, supports chaining calls
   */
  on<T extends MessageStreamEvent>(
    event: MessageStreamEventType,
    listener: EventListener<T>,
  ): this {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event)!.push({ listener: listener as EventListener<unknown>, once: false });
    return this;
  }

  /**
   * Remove event listener
   * @param event  Event type
   * @param listener  Listener function
   * @returns this, supports chaining calls
   */
  off<T extends MessageStreamEvent>(
    event: MessageStreamEventType,
    listener: EventListener<T>,
  ): this {
    const eventListeners = this.listeners.get(event);
    if (!eventListeners) {
      return this;
    }

    const index = eventListeners.findIndex(l => l.listener === listener);
    if (index !== -1) {
      eventListeners.splice(index, 1);
    }
    return this;
  }

  /**
   * Add a one-time event listener
   * @param event  Event type
   * @param listener  Listener function
   * @returns this, supports chaining calls
   */
  once<T extends MessageStreamEvent>(
    event: MessageStreamEventType,
    listener: EventListener<T>,
  ): this {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event)!.push({ listener: listener as EventListener<unknown>, once: true });
    return this;
  }

  /**
   * Wait for the event to be triggered.
   * @param event: Type of the event
   * @returns: A Promise that will resolve when the event is triggered
   */
  emitted<T extends MessageStreamEvent>(event: MessageStreamEventType): Promise<T> {
    return new Promise((resolve, reject) => {
      if (event !== "error") {
        this.once("error", (error: MessageStreamErrorEvent) => {
          reject(error.error);
        });
      }

      this.once(event, (data: T) => {
        resolve(data);
      });
    });
  }

  /**
   * Get the final message
   * @returns Promise, resolves to the final message when the stream ends
   */
  async finalMessage(): Promise<LLMMessage> {
    await this.done();

    if (this.receivedMessages.length === 0) {
      throw new ExecutionError("No messages received");
    }

    const lastMessage = this.receivedMessages[this.receivedMessages.length - 1];
    if (!lastMessage) {
      throw new ExecutionError("No final message available");
    }

    return lastMessage;
  }

  /**
   * Get the final text
   * @returns Promise, resolves to the final text when the stream ends
   */
  async finalText(): Promise<string> {
    const message = await this.finalMessage();

    if (typeof message.content === "string") {
      return message.content;
    }

    if (Array.isArray(message.content)) {
      return message.content
        .filter(item => item.type === "text")
        .map(item => item.text)
        .join("");
    }

    return "";
  }

  /**
   * Get the final result
   * @returns Promise, which resolves to the final result when the stream ends
   */
  async getFinalResult(): Promise<LLMResult> {
    await this.done();

    if (!this.finalResultValue) {
      throw new ExecutionError("No final result available");
    }

    return this.finalResultValue;
  }

  /**
   * Wait for the stream to end.
   * @returns A Promise that will be resolved when the stream ends.
   */
  done(): Promise<void> {
    this.catchingPromiseCreated = true;
    return this.endPromise;
  }

  /**
   * Push text increment
   * @param delta Text increment
   */
  pushText(delta: string): void {
    if (this.ended || this.errored || this.aborted) {
      return;
    }

    this.currentTextSnapshot += delta;
    this.emit("text", {
      type: "text",
      delta,
      snapshot: this.currentTextSnapshot,
    } as MessageStreamTextEvent);
  }

  /**
   * End stream (completed normally)
   */
  end(): void {
    if (this.ended || this.errored || this.aborted) {
      return;
    }

    // Trigger the finalMessage event (only when the process ends normally)
    if (this.receivedMessages.length > 0) {
      const lastMessage = this.receivedMessages[this.receivedMessages.length - 1];
      this.emit("finalMessage", {
        type: "finalMessage",
        message: lastMessage,
      } as MessageStreamFinalMessageEvent);
    }

    this.emit("end", {} as MessageStreamEndEvent);
  }

  /**
   * Stop the stream
   */
  abort(): void {
    if (this.aborted || this.ended) {
      return;
    }

    this.controller.abort();

    // Trigger the abort event
    this.emit("abort", {
      type: "abort",
      reason: "Stream aborted by user",
    } as MessageStreamAbortEvent);
  }

  /**
   * Split the stream into two separate streams
   * @returns Two separate streams
   */
  tee(): [MessageStream, MessageStream] {
    const leftQueue: InternalStreamEvent[] = [];
    const rightQueue: InternalStreamEvent[] = [];

    const iterator = this[Symbol.asyncIterator]();

    const createTeeStream = (queue: InternalStreamEvent[]): MessageStream => {
      const stream = new MessageStream();
      stream.controller = this.controller;
      stream.requestId = this.requestId;

      const teeIterator: AsyncIterator<InternalStreamEvent> = {
        async next(): Promise<IteratorResult<InternalStreamEvent>> {
          if (queue.length > 0) {
            return { value: queue.shift()!, done: false };
          }

          const result = await iterator.next();
          if (result.done) {
            return { value: undefined, done: true };
          }

          leftQueue.push(result.value);
          rightQueue.push(result.value);
          return { value: queue.shift()!, done: false };
        },

        async return(): Promise<IteratorResult<InternalStreamEvent>> {
          return { value: undefined, done: true };
        },
      };

      // Add an iterator to the stream.
      (stream as { [Symbol.asyncIterator]?: () => AsyncIterator<InternalStreamEvent> })[
        Symbol.asyncIterator
      ] = () => teeIterator;

      return stream;
    };

    return [createTeeStream(leftQueue), createTeeStream(rightQueue)];
  }

  /**
   * Fire an event
   * @param event  Event type
   * @param data  Event data
   */
  private emit<T = unknown>(event: MessageStreamEventType, data: T): void {
    if (this.ended) {
      return;
    }

    // Handle the end event
    if (event === "end") {
      this.ended = true;
      this.endPromiseResolve();
    }

    const eventListeners = this.listeners.get(event);
    if (!eventListeners || eventListeners.length === 0) {
      return;
    }

    // Filter one-time listeners
    const persistentListeners: FlaggedEventListener<unknown>[] = [];
    for (const listener of eventListeners) {
      try {
        // Expand parameters based on the event type.
        switch (event) {
          case "connect": {
            (listener.listener as () => void)();
            break;
          }
          case "streamEvent": {
            const streamEventData = data as MessageStreamStreamEvent;
            (
              listener.listener as (
                event: { type: string; data: unknown },
                snapshot: LLMMessage,
              ) => void
            )(streamEventData.event, streamEventData.snapshot);
            break;
          }
          case "text": {
            const textData = data as MessageStreamTextEvent;
            (listener.listener as (delta: string, snapshot: string) => void)(
              textData.delta,
              textData.snapshot,
            );
            break;
          }
          case "inputJson": {
            const inputJsonData = data as MessageStreamInputJsonEvent;
            (
              listener.listener as (
                partialJson: string,
                parsedSnapshot: unknown,
                snapshot: LLMMessage,
              ) => void
            )(inputJsonData.partialJson, inputJsonData.parsedSnapshot, inputJsonData.snapshot);
            break;
          }
          case "message": {
            const messageData = data as MessageStreamMessageEvent;
            (listener.listener as (message: LLMMessage) => void)(messageData.message);
            break;
          }
          case "finalMessage": {
            const finalMessageData = data as MessageStreamFinalMessageEvent;
            (listener.listener as (message: LLMMessage) => void)(finalMessageData.message);
            break;
          }
          case "error": {
            const errorData = data as MessageStreamErrorEvent;
            (listener.listener as (error: Error) => void)(errorData.error);
            break;
          }
          case "abort": {
            const abortData = data as MessageStreamAbortEvent;
            (listener.listener as (reason?: string) => void)(abortData.reason);
            break;
          }
          case "end": {
            (listener.listener as () => void)();
            break;
          }
          default:
            listener.listener(data);
        }

        if (!listener.once) {
          persistentListeners.push(listener);
        }
      } catch (error) {
        // The exception thrown by a listener does not affect other listeners.
        logger.error(`Error in event listener for ${event}`, {
          event,
          error: getErrorOrNew(error),
        });
      }
    }
    this.listeners.set(event, persistentListeners);

    // Handle the abort event
    if (event === "abort") {
      this.aborted = true;
      if (!this.catchingPromiseCreated && eventListeners.length === 0) {
        // Trigger an unhandled Promise error.
        setTimeout(() => {
          throw new ExecutionError("Stream aborted without error handler");
        }, 0);
      }
      this.endPromiseReject(new Error("Stream aborted"));
      this.emit("end", {} as MessageStreamEndEvent);
      return;
    }

    // Handle error events
    if (event === "error") {
      this.errored = true;
      if (!this.catchingPromiseCreated && eventListeners.length === 0) {
        // Trigger an unhandled Promise error.
        setTimeout(() => {
          throw (data as MessageStreamErrorEvent).error;
        }, 0);
      }
      this.endPromiseReject((data as MessageStreamErrorEvent).error);
      this.emit("end", {} as MessageStreamEndEvent);
    }
  }

  /**
   * Cumulative Messages
   * @param event Stream event
   * @returns Message snapshot
   */
  accumulateMessage(event: InternalStreamEvent): LLMMessage | null {
    switch (event.type) {
      case "message_start":
        // Trigger the connect event (only on the first occurrence of message_start).
        if (!this.currentMessageSnapshot && this.receivedMessages.length === 0) {
          this.emit("connect", {
            type: "connect",
          } as MessageStreamConnectEvent);
        }

        if (this.currentMessageSnapshot) {
          throw new ExecutionError("Message already started");
        }
        this.currentMessageSnapshot = {
          role: "assistant",
          content: "",
          ...(event.data as { message: Record<string, unknown> }).message,
        };
        this.currentTextSnapshot = "";
        // Trigger the streamEvent.
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: this.currentMessageSnapshot,
        } as MessageStreamStreamEvent);
        break;

      case "message_delta":
        if (!this.currentMessageSnapshot) {
          throw new ExecutionError("No message in progress");
        }
        {
          const data = event.data as {
            delta?: { stop_reason?: string; stop_sequence?: string };
            usage?: Record<string, unknown>;
          };
          if (data.delta?.stop_reason) {
            (this.currentMessageSnapshot as { stop_reason?: string }).stop_reason =
              data.delta.stop_reason;
          }
          if (data.delta?.stop_sequence) {
            (this.currentMessageSnapshot as { stop_sequence?: string }).stop_sequence =
              data.delta.stop_sequence;
          }
          if (data.usage) {
            // Merge the usage fields instead of overwriting them directly.
            const currentUsage =
              (this.currentMessageSnapshot as { usage?: Record<string, unknown> }).usage || {};
            (this.currentMessageSnapshot as { usage?: Record<string, unknown> }).usage = {
              ...currentUsage,
              ...data.usage,
            };
          }
        }
        // Trigger the streamEvent event
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: this.currentMessageSnapshot,
        } as MessageStreamStreamEvent);
        break;

      case "content_block_start":
        if (!this.currentMessageSnapshot) {
          throw new ExecutionError("No message in progress");
        }
        if (!Array.isArray(this.currentMessageSnapshot.content)) {
          this.currentMessageSnapshot.content = [];
        }
        {
          const data = event.data as {
            content_block: {
              type: "text" | "image_url" | "tool_use" | "tool_result" | "thinking";
            } & Record<string, unknown>;
          };
          this.currentMessageSnapshot.content.push({
            ...data.content_block,
          } as { type: "text" | "image_url" | "tool_use" | "tool_result" | "thinking" } & Record<
            string,
            unknown
          >);
        }
        // Trigger the streamEvent event
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: this.currentMessageSnapshot,
        } as MessageStreamStreamEvent);
        break;

      case "content_block_delta": {
        if (!this.currentMessageSnapshot) {
          throw new ExecutionError("No message in progress");
        }
        if (!Array.isArray(this.currentMessageSnapshot.content)) {
          break;
        }
        {
          const data = event.data as {
            index?: number;
            delta: {
              type: string;
              text?: string;
              citation?: unknown;
              partial_json?: string;
              thinking?: string;
              signature?: string;
            };
          };
          // Use `event.index` to locate the content block (if it exists).
          const blockIndex =
            data.index !== undefined ? data.index : this.currentMessageSnapshot.content.length - 1;
          const targetBlock = this.currentMessageSnapshot.content.at(blockIndex);
          if (!targetBlock) break;

          if (data.delta.type === "text_delta") {
            if (targetBlock.type === "text") {
              targetBlock.text += data.delta.text ?? "";
              this.currentTextSnapshot += data.delta.text ?? "";
              // Trigger the incremental text event
              this.emit("text", {
                type: "text",
                delta: data.delta.text ?? "",
                snapshot: this.currentTextSnapshot,
              } as MessageStreamTextEvent);
            }
          } else if (data.delta.type === "citations_delta") {
            // Handle reference increments
            if (targetBlock.type === "text") {
              const textBlock = targetBlock as { citations?: unknown[] };
              if (!textBlock.citations) {
                textBlock.citations = [];
              }
              textBlock.citations.push(data.delta.citation);
            }
          } else if (data.delta.type === "input_json_delta") {
            if (targetBlock.type === "tool_use") {
              // Use non-enumerated attributes to store the original JSON string (inspired by the Anthropic SDK design)
              const JSON_BUF_PROPERTY = "__json_buf";
              let jsonBuf = (targetBlock as { __json_buf?: string })[JSON_BUF_PROPERTY] || "";
              jsonBuf += data.delta.partial_json ?? "";

              // Update non-enumerated properties
              Object.defineProperty(targetBlock, JSON_BUF_PROPERTY, {
                value: jsonBuf,
                enumerable: false,
                writable: true,
                configurable: true,
              });

              // Use `partialParse` to parse incomplete JSON.
              const parsedInput = partialParse(jsonBuf);
              if (parsedInput !== undefined) {
                (targetBlock as { input?: unknown }).input = parsedInput;
              }

              // Trigger the `inputJson` event and provide real-time parsing results.
              this.emit("inputJson", {
                type: "inputJson",
                partialJson: jsonBuf,
                parsedSnapshot: parsedInput ?? jsonBuf,
                snapshot: this.currentMessageSnapshot,
              } as MessageStreamInputJsonEvent);
            }
          } else if (data.delta.type === "thinking_delta") {
            // Processing incremental thoughts
            if ((targetBlock as { type?: string }).type === "thinking") {
              const thinkingBlock = targetBlock as { thinking?: string };
              thinkingBlock.thinking = (thinkingBlock.thinking ?? "") + (data.delta.thinking ?? "");
            }
          } else if (data.delta.type === "signature_delta") {
            // Handle signatures
            if ((targetBlock as { type?: string }).type === "thinking") {
              const thinkingBlock = targetBlock as { signature?: string };
              thinkingBlock.signature = data.delta.signature ?? "";
            }
          }
        }
        // Trigger the streamEvent.
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: this.currentMessageSnapshot,
        } as MessageStreamStreamEvent);
        break;
      }

      case "content_block_stop": {
        // Clean up the non-enumerated properties of tool_use (from the original JSON buffer)
        if (this.currentMessageSnapshot && Array.isArray(this.currentMessageSnapshot.content)) {
          const data = event.data as { index?: number };
          const blockIndex =
            data.index !== undefined ? data.index : this.currentMessageSnapshot.content.length - 1;
          const targetBlock = this.currentMessageSnapshot.content.at(blockIndex);
          if (targetBlock && targetBlock.type === "tool_use") {
            const JSON_BUF_PROPERTY = "__json_buf";
            // Remove non-enumerated attributes to free up memory.
            if (JSON_BUF_PROPERTY in targetBlock) {
              delete (targetBlock as { __json_buf?: string })[JSON_BUF_PROPERTY];
            }
          }
        }
        // Trigger the streamEvent.
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: this.currentMessageSnapshot,
        } as MessageStreamStreamEvent);
        break;
      }

      case "message_stop": {
        const message = this.currentMessageSnapshot;
        if (message) {
          this.receivedMessages.push(message);
          this.currentMessageSnapshot = null;
          this.currentTextSnapshot = "";

          // Trigger the message event
          this.emit("message", {
            type: "message",
            message,
          } as MessageStreamMessageEvent);
        }
        // Trigger the streamEvent.
        this.emit("streamEvent", {
          type: "streamEvent",
          event: { type: event.type, data: event.data },
          snapshot: message || { role: "assistant", content: "" },
        } as MessageStreamStreamEvent);
        return message;
      }

      default:
        break;
    }

    return this.currentMessageSnapshot;
  }

  /**
   * Set the final result
   * @param result The final result
   */
  setFinalResult(result: LLMResult): void {
    this.finalResultValue = result;
  }

  /**
   * Implementation of the AsyncIterable interface
   */
  [Symbol.asyncIterator](): AsyncIterator<InternalStreamEvent> {
    // Add an event listener
    this.on("streamEvent", (event: MessageStreamStreamEvent) => {
      const internalEvent: InternalStreamEvent = {
        type: event.event.type,
        data: event.event.data,
      };
      if (this.readQueue.length > 0) {
        const reader = this.readQueue.shift()!;
        reader(internalEvent);
      } else {
        this.pushQueue.push(internalEvent);
      }
    });

    this.once("end", () => {
      while (this.readQueue.length > 0) {
        const reader = this.readQueue.shift()!;
        reader({ type: "end", data: undefined });
      }
    });

    this.once("abort", () => {
      while (this.readQueue.length > 0) {
        const reader = this.readQueue.shift()!;
        reader({ type: "abort", data: undefined });
      }
    });

    this.once("error", (error: MessageStreamErrorEvent) => {
      while (this.readQueue.length > 0) {
        const reader = this.readQueue.shift()!;
        reader({ type: "error", data: error });
      }
    });

    return {
      next: async (): Promise<IteratorResult<InternalStreamEvent>> => {
        if (this.pushQueue.length > 0) {
          return { value: this.pushQueue.shift()!, done: false };
        }

        if (this.ended || this.errored || this.aborted) {
          return { value: undefined, done: true };
        }

        return new Promise(resolve => {
          this.readQueue.push((event: InternalStreamEvent) => {
            resolve({ value: event, done: false });
          });
        });
      },

      return: async (): Promise<IteratorResult<InternalStreamEvent>> => {
        this.abort();
        return { value: undefined, done: true };
      },
    };
  }

  /**
   * Set the response object
   * @param response Response object
   */
  setResponse(response: Response): void {
    this.response = response;
  }

  /**
   * Set the request ID
   * @param requestId Request ID
   */
  setRequestId(requestId: string): void {
    this.requestId = requestId;
  }

  /**
   * Get the request ID
   * @returns Request ID
   */
  getRequestId(): string | null {
    return this.requestId;
  }

  /**
   * Get the response object
   * @returns Response object
   */
  getResponse(): Response | null {
    return this.response;
  }

  /**
   * Check if it has ended.
   * @returns Whether it has ended
   */
  isEnded(): boolean {
    return this.ended;
  }

  /**
   * Check for any errors
   * @returns Whether there are any errors
   */
  isErrored(): boolean {
    return this.errored;
  }

  /**
   * Check if it has been aborted.
   * @returns Whether it has been aborted.
   */
  isAborted(): boolean {
    return this.aborted;
  }

  /**
   * Get the received messages
   * @returns Array of received messages
   */
  getReceivedMessages(): LLMMessage[] {
    return [...this.receivedMessages];
  }

  /**
   * Get the AbortController
   * @returns AbortController
   */
  getController(): AbortController {
    return this.controller;
  }
}
