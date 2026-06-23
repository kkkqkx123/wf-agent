/**
 * Message base type definition
 * Defining the core data structure of a message
 */

/**
 * message role
 */
export type MessageRole = "system" | "user" | "assistant" | "tool";

/**
 * Reference location type
 */
export type CitationLocationType =
  | "char_location"
  | "page_location"
  | "content_block_location"
  | "web_search_result_location"
  | "search_result_location";

/**
 * Reference Location Base Interface
 */
export interface CitationLocation {
  /** Quoted text */
  cited_text: string;
  /** Document Indexing */
  document_index: number;
  /** Document Title */
  document_title: string | null;
  /** Location Type */
  type: CitationLocationType;
  /** Document ID (optional) */
  file_id?: string;
}

/**
 * Character position reference
 */
export interface CitationCharLocation extends CitationLocation {
  type: "char_location";
  /** Starting character index */
  start_char_index: number;
  /** End character index */
  end_char_index: number;
}

/**
 * Page location references
 */
export interface CitationPageLocation extends CitationLocation {
  type: "page_location";
  /** Starting Page Code */
  start_page_number: number;
  /** End Page */
  end_page_number: number;
}

/**
 * Content block location references
 */
export interface CitationContentBlockLocation extends CitationLocation {
  type: "content_block_location";
  /** Starting Block Index */
  start_block_index: number;
  /** End Block Index */
  end_block_index: number;
}

/**
 * Web search result location references
 */
export interface CitationWebSearchResultLocation extends CitationLocation {
  type: "web_search_result_location";
  /** Encrypted content */
  encrypted_content: string;
  /** URL */
  url: string;
  /** Page age (optional) */
  page_age?: string | null;
}

/**
 * Search results location references
 */
export interface CitationSearchResultLocation extends CitationLocation {
  type: "search_result_location";
  /** Starting Block Index */
  start_block_index: number;
  /** Search results indexing */
  search_result_index: number;
  /** source (of information etc) */
  source: string;
  /** caption */
  title: string | null;
}

/**
 * reference type
 */
export type TextCitation =
  | CitationCharLocation
  | CitationPageLocation
  | CitationContentBlockLocation
  | CitationWebSearchResultLocation
  | CitationSearchResultLocation;

/**
 * Message content type
 */
export type MessageContent =
  | string
  | Array<{
      type: "text" | "image_url" | "tool_use" | "tool_result" | "thinking";
      text?: string;
      /** List of references (only for text type) */
      citations?: TextCitation[];
      image_url?: { url: string };
      tool_use?: {
        id: string;
        name: string;
        input: Record<string, unknown> | string; // String support when streaming
      };
      tool_result?: {
        tool_use_id: string;
        content: string | Array<{ type: string; text: string }>;
      };
      /** Thinking content (only for thinking type) */
      thinking?: string;
      /** Signature (for thinking type only) */
      signature?: string;
    }>;

/**
 * Message Base Interface
 */
export interface Message {
  /** message role */
  role: MessageRole;
  /** Message */
  content: MessageContent;
  /** Message ID (optional) */
  id?: string;
  /** Message timestamp (optional) */
  timestamp?: number;
  /** Other metadata */
  metadata?: Record<string, unknown>;
}

/**
 * LLM tool call types
 */
export interface LLMToolCall {
  /** Tool Call ID */
  id: string;
  /** Type (function) */
  type: "function";
  /** Function Call Information */
  function: {
    /** function name */
    name: string;
    /** Function parameters (JSON strings) */
    arguments: string;
  };
}

/**
 * LLM message interface (extends base message, adds LLM-specific properties)
 */
export interface LLMMessage extends Message {
  /** Extended Thinking (for ASSISTANT role only) */
  thinking?: string;
  /** Tool invocation array (assistant role) */
  toolCalls?: LLMToolCall[];
  /** Tool invocation ID (tool role) */
  toolCallId?: string;
}
