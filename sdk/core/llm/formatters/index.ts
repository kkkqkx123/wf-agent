/**
 * Formatter Module Export
 *
 * Provides a unified export interface for format converters
 */

// Type Export
export type {
  HttpRequestOptions,
  StreamChunk,
  FormatterConfig,
  BuildRequestResult,
  ParseResponseResult,
  ParseStreamChunkResult,
  AuthType,
  CustomHeader,
  CustomBodyConfig,
  ToolCallParseOptions,
} from "./types.js";

// Base class export
export { BaseFormatter } from "./base.js";

// Tool Call Parser Export
export {
  ToolCallParser,
  type XMLToolCall,
  type JSONToolCall,
} from "./tool-call-parser.js";

// Tool Format Selector Export
export {
  getToolFormatTemplates,
  getToolCallParserOptions,
  resolveToolCallFormatConfig,
  requiresPromptToolDescriptions,
  requiresCustomParsing,
  validateToolFormatCompatibility,
  getToolFormatDisplayName,
  getToolFormatDescription,
  getAvailableToolFormats,
  type ToolFormatTemplateSet,
} from "./tool-format-selector.js";

// OpenAI Format Converter
export { OpenAIChatFormatter } from "./openai-chat.js";
export { OpenAIResponseFormatter } from "./openai-response.js";

// Anthropic Format Converter
export { AnthropicFormatter } from "./anthropic.js";

// Gemini Format Converter
export { GeminiNativeFormatter } from "./gemini-native.js";
export { GeminiOpenAIFormatter } from "./gemini-openai.js";

// Registry Export
export {
  FormatterRegistry,
  formatterRegistry,
  getFormatter,
  registerFormatter,
} from "./registry.js";
