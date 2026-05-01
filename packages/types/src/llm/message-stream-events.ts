/**
 * MessageStream event type definition
 * Define event types related to message streams
 * 
 * These events represent the streaming response from LLM providers
 * and are used by the Agent layer for real-time processing.
 */

import type { LLMMessage } from "../message/index.js";

/**
 * Message Stream Event Types
 */
export type MessageStreamEventType =
  | "connect" /** Connection established */
  | "streamEvent" /** Streaming events (raw events + snapshots) */
  | "text" /** Text increment */
  | "inputJson" /** Real-time analysis of tool parameters */
  | "message" /** Full message reception */
  | "finalMessage" /** Final message confirmation */
  | "error" /** Error occurred */
  | "abort" /** Stream aborted */
  | "end"; /** Stream ended */

/**
 * Message Flow Event Types
 * Union of all possible message stream events
 */
export type MessageStreamEvent =
  | MessageStreamConnectEvent
  | MessageStreamStreamEvent
  | MessageStreamTextEvent
  | MessageStreamInputJsonEvent
  | MessageStreamMessageEvent
  | MessageStreamFinalMessageEvent
  | MessageStreamErrorEvent
  | MessageStreamAbortEvent
  | MessageStreamEndEvent;

/**
 * Message stream connection event
 * Emitted when the stream connection is established
 */
export interface MessageStreamConnectEvent {
  type: "connect";
}

/**
 * Connect event listener type
 */
export type ConnectEventListener = () => void;

/**
 * Message flow events (original events + snapshots)
 * Provides access to raw provider events with accumulated message snapshot
 */
export interface MessageStreamStreamEvent {
  type: "streamEvent";
  event: {
    type: string;
    data: unknown;
  };
  snapshot: LLMMessage;
}

/**
 * streamEvent event listener type
 */
export type StreamEventListener = (
  event: { type: string; data: unknown },
  snapshot: LLMMessage,
) => void;

/**
 * Message Stream Text Incremental Events
 * Emitted for each text delta received from the LLM
 */
export interface MessageStreamTextEvent {
  type: "text";
  delta: string;
  snapshot: string;
}

/**
 * Text Event Listener Type
 */
export type TextEventListener = (delta: string, snapshot: string) => void;

/**
 * Message Flow Tool Parameter Real-time Parsing Event
 * Emitted as JSON tool arguments are being parsed in real-time
 */
export interface MessageStreamInputJsonEvent {
  type: "inputJson";
  partialJson: string;
  parsedSnapshot: unknown;
  snapshot: LLMMessage;
}

/**
 * `inputJson` Event Listener Type
 */
export type InputJsonEventListener = (
  partialJson: string,
  parsedSnapshot: unknown,
  snapshot: LLMMessage,
) => void;

/**
 * Message Flow Complete Message Event
 * Emitted when a complete message is received
 */
export interface MessageStreamMessageEvent {
  type: "message";
  message: LLMMessage;
}

/**
 * Message Event Listener Type
 */
export type MessageEventListener = (message: LLMMessage) => void;

/**
 * Message Flow Final Message Event
 * Emitted when the stream completes with the final message
 */
export interface MessageStreamFinalMessageEvent {
  type: "finalMessage";
  message: LLMMessage;
}

/**
 * `finalMessage` event listener type
 */
export type FinalMessageEventListener = (message: LLMMessage) => void;

/**
 * Message flow error event
 * Emitted when an error occurs during streaming
 */
export interface MessageStreamErrorEvent {
  type: "error";
  error: Error;
}

/**
 * Error event listener type
 */
export type ErrorEventListener = (error: Error) => void;

/**
 * Message Flow Termination Event
 * Emitted when the stream is aborted
 */
export interface MessageStreamAbortEvent {
  type: "abort";
  reason?: string;
}

/**
 * abort Event Listener Type
 */
export type AbortEventListener = (reason?: string) => void;

/**
 * Message Stream End Event
 * Emitted when the stream ends normally
 */
export interface MessageStreamEndEvent {
  type: "end";
}

/**
 * End event listener type
 */
export type EndEventListener = () => void;
