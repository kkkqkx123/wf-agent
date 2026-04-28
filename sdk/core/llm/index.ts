/**
 * SDK Core LLM Module
 *
 * Provides a unified interface for calling LLMs, supporting multiple LLM providers.
 * Uses the Formatter strategy pattern to handle format conversions from different providers.
 */

// SDK-level core classes
export { LLMWrapper } from "./wrapper.js";
export { ProfileManager } from "./profile-manager.js";

// Infrastructure
export { ClientFactory } from "./client-factory.js";
export { BaseLLMClient } from "./base-client.js";
export { MessageStream } from "./message-stream.js";

// Formatter-related exports
export {
  BaseFormatter,
  OpenAIChatFormatter,
  OpenAIResponseFormatter,
  AnthropicFormatter,
  GeminiNativeFormatter,
  GeminiOpenAIFormatter,
  FormatterRegistry,
  formatterRegistry,
  getFormatter,
  registerFormatter,
  type HttpRequestOptions,
  type StreamChunk,
  type FormatterConfig,
  type BuildRequestResult,
  type ParseResponseResult,
  type ParseStreamChunkResult,
} from "./formatters/index.js";

// Event Type
export {
  MessageStreamEventType,
  type MessageStreamEvent,
  type MessageStreamStreamEvent,
  type MessageStreamTextEvent,
  type MessageStreamInputJsonEvent,
  type MessageStreamMessageEvent,
  type MessageStreamFinalMessageEvent,
  type MessageStreamErrorEvent,
  type MessageStreamAbortEvent,
  type MessageStreamEndEvent,
} from "./message-stream-events.js";

// Tool functions
export * from "./message-helper.js";

// OpenAI Client
export { OpenAIChatClient } from "./clients/openai-chat.js";
export { OpenAIResponseClient } from "./clients/openai-response.js";

// Anthropic Client
export { AnthropicClient } from "./clients/anthropic.js";

// Gemini Client
export { GeminiNativeClient } from "./clients/gemini-native.js";
export { GeminiOpenAIClient } from "./clients/gemini-openai.js";
