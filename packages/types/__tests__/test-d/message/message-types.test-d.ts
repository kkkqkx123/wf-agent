/**
 * Message Type Tests
 * 
 * Tests for message types including:
 * - Message base structure
 * - MessageRole literal types
 * - MessageContent (string and array formats)
 * - LLMMessage with tool calls
 * - Citation types (5 different location types)
 * - LLMToolCall structure
 * 
 * Priority: MEDIUM (Phase 2)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  Message,
  MessageRole,
  MessageContent,
  LLMMessage,
  LLMToolCall,
} from "../../../src/index.js";
// Citation types are not exported from index, import directly from message.ts
import type {
  TextCitation,
  CitationCharLocation,
  CitationPageLocation,
  CitationContentBlockLocation,
  CitationWebSearchResultLocation,
  CitationSearchResultLocation,
} from "../../../src/message/message.js";

// =============================================================================
// Test 1: MessageRole Literal Types
// =============================================================================

const systemRole: MessageRole = "system";
const userRole: MessageRole = "user";
const assistantRole: MessageRole = "assistant";
const toolRole: MessageRole = "tool";

expectAssignable<MessageRole>(systemRole);
expectAssignable<MessageRole>(userRole);
expectAssignable<MessageRole>(assistantRole);
expectAssignable<MessageRole>(toolRole);

// =============================================================================
// Test 2: Message Base Structure
// =============================================================================

const simpleMessage: Message = {
  role: "user",
  content: "Hello, how are you?",
};

expectType<Message>(simpleMessage);
expectType<MessageRole>(simpleMessage.role);
expectType<MessageContent>(simpleMessage.content);
expectType<string | undefined>(simpleMessage.id);
expectType<number | undefined>(simpleMessage.timestamp);
expectType<Record<string, unknown> | undefined>(simpleMessage.metadata);

// Message with all fields
const fullMessage: Message = {
  role: "assistant",
  content: "I'm doing well, thank you!",
  id: "msg-123",
  timestamp: Date.now(),
  metadata: {
    source: "llm",
    model: "gpt-4",
  },
};

expectType<Message>(fullMessage);
expectType<string>(fullMessage.id!);
expectType<number>(fullMessage.timestamp!);
expectType<Record<string, unknown>>(fullMessage.metadata!);

// =============================================================================
// Test 3: MessageContent as String
// =============================================================================

const stringContent: MessageContent = "This is a simple text message";

expectAssignable<MessageContent>(stringContent);
expectType<string>(stringContent);

// Assign to message
const textMessage: Message = {
  role: "user",
  content: stringContent,
};

expectType<Message>(textMessage);

// =============================================================================
// Test 4: MessageContent as Array (Multi-modal)
// =============================================================================

const arrayContent: MessageContent = [
  {
    type: "text",
    text: "Here's an image:",
  },
  {
    type: "image_url",
    image_url: { url: "https://example.com/image.png" },
  },
];

expectAssignable<MessageContent>(arrayContent);

// Message with array content
const multiModalMessage: Message = {
  role: "user",
  content: arrayContent,
};

expectType<Message>(multiModalMessage);

// =============================================================================
// Test 5: Text Content Block with Citations
// =============================================================================

const textWithCitations: MessageContent = [
  {
    type: "text",
    text: "According to the document...",
    citations: [
      {
        type: "char_location",
        cited_text: "important information",
        document_index: 0,
        document_title: "Document.pdf",
        start_char_index: 100,
        end_char_index: 120,
      },
    ],
  },
];

expectAssignable<MessageContent>(textWithCitations);

// =============================================================================
// Test 6: Tool Use Content Block
// =============================================================================

const toolUseContent: MessageContent = [
  {
    type: "tool_use",
    tool_use: {
      id: "tool-call-123",
      name: "search",
      input: { query: "weather forecast" },
    },
  },
];

expectAssignable<MessageContent>(toolUseContent);

// Tool use with string input (streaming)
const streamingToolUse: MessageContent = [
  {
    type: "tool_use",
    tool_use: {
      id: "tool-call-456",
      name: "read_file",
      input: '{"path": "/test.txt"}', // JSON string during streaming
    },
  },
];

expectAssignable<MessageContent>(streamingToolUse);

// =============================================================================
// Test 7: Tool Result Content Block
// =============================================================================

const toolResultContent: MessageContent = [
  {
    type: "tool_result",
    tool_result: {
      tool_use_id: "tool-call-123",
      content: "The weather is sunny",
    },
  },
];

expectAssignable<MessageContent>(toolResultContent);

// Tool result with structured content
const structuredToolResult: MessageContent = [
  {
    type: "tool_result",
    tool_result: {
      tool_use_id: "tool-call-456",
      content: [
        { type: "text", text: "File contents" },
        { type: "code", text: "console.log('hello')" },
      ],
    },
  },
];

expectAssignable<MessageContent>(structuredToolResult);

// =============================================================================
// Test 8: Thinking Content Block
// =============================================================================

const thinkingContent: MessageContent = [
  {
    type: "thinking",
    thinking: "Let me analyze this step by step...",
    signature: "thinking-signature-abc123",
  },
];

expectAssignable<MessageContent>(thinkingContent);

// =============================================================================
// Test 9: CitationCharLocation
// =============================================================================

const charCitation: CitationCharLocation = {
  type: "char_location",
  cited_text: "quoted text",
  document_index: 0,
  document_title: "document.pdf",
  start_char_index: 50,
  end_char_index: 100,
  file_id: "file-123",
};

expectType<CitationCharLocation>(charCitation);
expectType<"char_location">(charCitation.type);
expectType<string>(charCitation.cited_text);
expectType<number>(charCitation.document_index);
expectType<string | null>(charCitation.document_title);
expectType<number>(charCitation.start_char_index);
expectType<number>(charCitation.end_char_index);
expectType<string | undefined>(charCitation.file_id);

// =============================================================================
// Test 10: CitationPageLocation
// =============================================================================

const pageCitation: CitationPageLocation = {
  type: "page_location",
  cited_text: "content from pages",
  document_index: 1,
  document_title: "book.pdf",
  start_page_number: 10,
  end_page_number: 15,
};

expectType<CitationPageLocation>(pageCitation);
expectType<"page_location">(pageCitation.type);
expectType<number>(pageCitation.start_page_number);
expectType<number>(pageCitation.end_page_number);

// =============================================================================
// Test 11: CitationContentBlockLocation
// =============================================================================

const blockCitation: CitationContentBlockLocation = {
  type: "content_block_location",
  cited_text: "block content",
  document_index: 2,
  document_title: null,
  start_block_index: 5,
  end_block_index: 10,
};

expectType<CitationContentBlockLocation>(blockCitation);
expectType<"content_block_location">(blockCitation.type);
expectType<number>(blockCitation.start_block_index);
expectType<number>(blockCitation.end_block_index);

// =============================================================================
// Test 12: CitationWebSearchResultLocation
// =============================================================================

const webSearchCitation: CitationWebSearchResultLocation = {
  type: "web_search_result_location",
  cited_text: "web content",
  document_index: 0,
  document_title: "Example Website",
  encrypted_content: "encrypted-data-here",
  url: "https://example.com/article",
  page_age: "2024-01-01",
};

expectType<CitationWebSearchResultLocation>(webSearchCitation);
expectType<"web_search_result_location">(webSearchCitation.type);
expectType<string>(webSearchCitation.encrypted_content);
expectType<string>(webSearchCitation.url);
expectType<string | null | undefined>(webSearchCitation.page_age);

// Without optional page_age
const webSearchCitationMinimal: CitationWebSearchResultLocation = {
  type: "web_search_result_location",
  cited_text: "web content",
  document_index: 0,
  document_title: null,
  encrypted_content: "encrypted",
  url: "https://example.com",
};

expectType<CitationWebSearchResultLocation>(webSearchCitationMinimal);

// =============================================================================
// Test 13: CitationSearchResultLocation
// =============================================================================

const searchCitation: CitationSearchResultLocation = {
  type: "search_result_location",
  cited_text: "search result snippet",
  document_index: 0,
  document_title: "Search Result Title",
  start_block_index: 0,
  search_result_index: 1,
  source: "Google",
  title: "Article Title",
};

expectType<CitationSearchResultLocation>(searchCitation);
expectType<"search_result_location">(searchCitation.type);
expectType<number>(searchCitation.start_block_index);
expectType<number>(searchCitation.search_result_index);
expectType<string>(searchCitation.source);
expectType<string | null>(searchCitation.title);

// =============================================================================
// Test 14: TextCitation Union Type
// =============================================================================

declare const anyCitation: TextCitation;

// Type narrowing based on type field
if (anyCitation.type === "char_location") {
  expectType<CitationCharLocation>(anyCitation);
  expectType<number>(anyCitation.start_char_index);
} else if (anyCitation.type === "page_location") {
  expectType<CitationPageLocation>(anyCitation);
  expectType<number>(anyCitation.start_page_number);
} else if (anyCitation.type === "content_block_location") {
  expectType<CitationContentBlockLocation>(anyCitation);
  expectType<number>(anyCitation.start_block_index);
} else if (anyCitation.type === "web_search_result_location") {
  expectType<CitationWebSearchResultLocation>(anyCitation);
  expectType<string>(anyCitation.url);
} else if (anyCitation.type === "search_result_location") {
  expectType<CitationSearchResultLocation>(anyCitation);
  expectType<number>(anyCitation.search_result_index);
}

// All citation types are assignable to TextCitation
const citations: TextCitation[] = [
  charCitation,
  pageCitation,
  blockCitation,
  webSearchCitation,
  searchCitation,
];

expectType<TextCitation[]>(citations);

// =============================================================================
// Test 15: LLMToolCall Structure
// =============================================================================

const toolCall: LLMToolCall = {
  id: "call-abc123",
  type: "function",
  function: {
    name: "get_weather",
    arguments: '{"location": "New York"}',
  },
};

expectType<LLMToolCall>(toolCall);
expectType<string>(toolCall.id);
expectType<"function">(toolCall.type);
expectType<string>(toolCall.function.name);
expectType<string>(toolCall.function.arguments);

// Complex arguments
const complexToolCall: LLMToolCall = {
  id: "call-def456",
  type: "function",
  function: {
    name: "search_database",
    arguments: JSON.stringify({
      query: "SELECT * FROM users",
      limit: 10,
      filters: { status: "active" },
    }),
  },
};

expectType<LLMToolCall>(complexToolCall);

// =============================================================================
// Test 16: LLMMessage Structure
// =============================================================================

const llmMessage: LLMMessage = {
  role: "assistant",
  content: "I'll help you with that.",
  thinking: "The user needs assistance with their request",
  toolCalls: [toolCall, complexToolCall],
};

expectType<LLMMessage>(llmMessage);
expectType<MessageRole>(llmMessage.role);
expectType<MessageContent>(llmMessage.content);
expectType<string | undefined>(llmMessage.thinking);
expectType<LLMToolCall[] | undefined>(llmMessage.toolCalls);
expectType<string | undefined>(llmMessage.toolCallId);

// Assistant message without tool calls
const assistantMessage: LLMMessage = {
  role: "assistant",
  content: "Simple response",
};

expectType<LLMMessage>(assistantMessage);

// Tool result message
const toolResultMessage: LLMMessage = {
  role: "tool",
  content: "Tool execution result",
  toolCallId: "call-abc123",
};

expectType<LLMMessage>(toolResultMessage);
expectType<string | undefined>(toolResultMessage.toolCallId);

// Message with thinking
const thinkingMessage: LLMMessage = {
  role: "assistant",
  content: "Final answer after reasoning",
  thinking: "Let me think through this problem step by step...",
};

expectType<LLMMessage>(thinkingMessage);
expectType<string | undefined>(thinkingMessage.thinking);

// =============================================================================
// Test 17: Integration Pattern - Message History
// =============================================================================

interface MessageHistory {
  messages: Message[];
  totalMessages: number;
  lastUserMessage?: Message;
  lastAssistantMessage?: LLMMessage;
}

const history: MessageHistory = {
  messages: [simpleMessage, fullMessage, llmMessage],
  totalMessages: 3,
  lastUserMessage: simpleMessage,
  lastAssistantMessage: llmMessage,
};

expectType<MessageHistory>(history);
expectType<Message[]>(history.messages);
expectType<number>(history.totalMessages);
expectType<Message | undefined>(history.lastUserMessage);
expectType<LLMMessage | undefined>(history.lastAssistantMessage);

// =============================================================================
// Test 18: Integration Pattern - Message Builder
// =============================================================================

interface MessageBuilder {
  role: MessageRole;
  content: MessageContent;
  build(): Message;
}

class SimpleMessageBuilder implements MessageBuilder {
  role: MessageRole;
  content: MessageContent;

  constructor(role: MessageRole, content: string) {
    this.role = role;
    this.content = content;
  }

  build(): Message {
    return {
      role: this.role,
      content: this.content,
      timestamp: Date.now(),
    };
  }
}

const builder = new SimpleMessageBuilder("user", "Test message");
const builtMessage = builder.build();

expectType<Message>(builtMessage);
expectType<MessageRole>(builtMessage.role);
expectType<MessageContent>(builtMessage.content);
