/**
 * MessageStream event type definition
 * Define event types related to message streams
 * Since it is necessary to import the LLM type and it is only for internal use, it is not suitable to include it in the global definitions.
 */

import type { LLMMessage } from "@wf-agent/types";

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
  | "error" /** incorrect */
  | "abort" /** discontinue */
  | "end"; /** close */

/**
 * Message Flow Event Types
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
 */
export interface MessageStreamConnectEvent {
  type: "connect";
}

/**
 * Connect event listener type (expanded parameters)
 */
export type ConnectEventListener = () => void;

/**
 * Message flow events (original events + snapshots)
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
 * streamEvent event listener type (expanded parameters)
 */
export type StreamEventListener = (
  event: { type: string; data: unknown },
  snapshot: LLMMessage,
) => void;

/**
 * Message Stream Text Incremental Events
 */
export interface MessageStreamTextEvent {
  type: "text";
  delta: string;
  snapshot: string;
}

/**
 * Text Event Listener Type (expanded parameters)
 */
export type TextEventListener = (delta: string, snapshot: string) => void;

/**
 * Message Flow Tool Parameter Real-time Parsing Event
 */
export interface MessageStreamInputJsonEvent {
  type: "inputJson";
  partialJson: string;
  parsedSnapshot: unknown;
  snapshot: LLMMessage;
}

/**
 * `inputJson` Event Listener Type (expanded parameters)
 */
export type InputJsonEventListener = (
  partialJson: string,
  parsedSnapshot: unknown,
  snapshot: LLMMessage,
) => void;

/**
 * Message Flow Complete Message Event
 */
export interface MessageStreamMessageEvent {
  type: "message";
  message: LLMMessage;
}

/**
 * Message Event Listener Type (expanded parameters)
 */
export type MessageEventListener = (message: LLMMessage) => void;

/**
 * Message Flow Final Message Event
 */
export interface MessageStreamFinalMessageEvent {
  type: "finalMessage";
  message: LLMMessage;
}

/**
 * `finalMessage` event listener type (expanded parameters)
 */
export type FinalMessageEventListener = (message: LLMMessage) => void;

/**
 * Message flow error event
 */
export interface MessageStreamErrorEvent {
  type: "error";
  error: Error;
}

/**
 * Error event listener type (expanded parameters)
 */
export type ErrorEventListener = (error: Error) => void;

/**
 * Message Flow Termination Event
 */
export interface MessageStreamAbortEvent {
  type: "abort";
  reason?: string;
}

/**
 * abort Event Listener Type (expanded parameters)
 */
export type AbortEventListener = (reason?: string) => void;

/**
 * Message Stream End Event
 */
export interface MessageStreamEndEvent {
  type: "end";
}

/**
 * End event listener type (expanded parameters)
 */
export type EndEventListener = () => void;
