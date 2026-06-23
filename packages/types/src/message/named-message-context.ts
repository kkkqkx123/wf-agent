/**
 * Named Message Context Type Definition
 * 
 * Core data structure for the simplified multi-context management architecture.
 * Replaces the complex template system with direct message array references.
 */

import type { LLMMessage } from "./message.js";
import type { Timestamp, ID } from "../common.js";

/**
 * Named Message Context
 * 
 * A semantically identified message collection that can be referenced by nodes.
 * Each context has a unique ID and contains an array of LLM messages.
 */
export interface NamedMessageContext {
  /** Semantic ID (e.g., 'current', 'system', 'research-notes') */
  id: string;
  
  /** Message array */
  messages: LLMMessage[];
  
  /** Creation timestamp */
  createdAt: Timestamp;
  
  /** Last update timestamp */
  updatedAt: Timestamp;
  
  /** Metadata */
  metadata?: {
    /** Human-readable description */
    description?: string;
    /** Source node ID that created this context */
    sourceNodeId?: ID;
    /** Tags for categorization */
    tags?: string[];
    /** Estimated token count */
    tokenCount?: number;
  };
}

/**
 * Message Context Registry
 * 
 * Manages the lifecycle of named message contexts within an execution.
 * Provides CRUD operations for context management.
 */
export interface MessageContextRegistry {
  /** Register a new context */
  register(context: NamedMessageContext): void;
  
  /** Get a context by ID */
  get(id: string): NamedMessageContext | undefined;
  
  /** Update messages in an existing context */
  update(id: string, messages: LLMMessage[]): void;
  
  /** Delete a context by ID */
  delete(id: string): void;
  
  /** List all registered context IDs */
  listIds(): string[];
  
  /** Check if a context exists */
  has(id: string): boolean;
}

/**
 * Built-in Context IDs
 * 
 * Reserved semantic IDs with special meaning in the execution context.
 */
export const BUILTIN_CONTEXT_IDS = {
  /** 
   * Current execution's main conversation context
   * 
   * Automatically maintained, contains all historical messages.
   * This is the default context for all nodes unless specified otherwise.
   */
  CURRENT: 'current',
  
  /** 
   * System instructions context
   * 
   * Typically contains system role messages.
   */
  SYSTEM: 'system',
  
  /** 
   * Temporary context for intermediate results
   * 
   * Can be overwritten by any node.
   */
  TEMP: 'temp',
} as const;

/**
 * Type for built-in context ID keys
 */
export type BuiltinContextId = typeof BUILTIN_CONTEXT_IDS[keyof typeof BUILTIN_CONTEXT_IDS];
