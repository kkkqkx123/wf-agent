/**
 * Async Context for Logger
 * Provides request/transaction-level tracing capabilities
 * Uses AsyncLocalStorage to propagate context across async operations
 */

import { AsyncLocalStorage } from "async_hooks";
import type { LoggerContext } from "./types.js";

/**
 * Async context storage for logger
 */
const asyncStorage = new AsyncLocalStorage<Map<string, unknown>>();

/**
 * Context key for trace ID
 */
export const TRACE_ID_KEY = "traceId";

/**
 * Context key for span ID
 */
export const SPAN_ID_KEY = "spanId";

/**
 * Context key for parent span ID
 */
export const PARENT_SPAN_ID_KEY = "parentSpanId";

/**
 * Generate a unique trace ID
 */
export function generateTraceId(): string {
  return `trace_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
}

/**
 * Generate a unique span ID
 */
export function generateSpanId(): string {
  return `span_${Math.random().toString(36).substring(2, 11)}`;
}

/**
 * Run a function within an async context
 * @param fn Function to run
 * @param context Initial context values
 * @returns Function result
 */
export function runWithContext<T>(fn: () => T, context: Record<string, unknown> = {}): T {
  const store = new Map<string, unknown>(Object.entries(context));
  return asyncStorage.run(store, fn);
}

/**
 * Run a function within an async context with a new trace
 * Automatically generates traceId and spanId
 */
export function runWithTrace<T>(fn: () => T, traceId?: string, spanId?: string): T {
  const context: Record<string, unknown> = {
    [TRACE_ID_KEY]: traceId || generateTraceId(),
    [SPAN_ID_KEY]: spanId || generateSpanId(),
  };
  return runWithContext(fn, context);
}

/**
 * Get a value from the current async context
 */
export function getContextValue<T>(key: string): T | undefined {
  const store = asyncStorage.getStore();
  if (!store) return undefined;
  return store.get(key) as T | undefined;
}

/**
 * Get the current trace ID from context
 */
export function getTraceId(): string | undefined {
  return getContextValue<string>(TRACE_ID_KEY);
}

/**
 * Get the current span ID from context
 */
export function getSpanId(): string | undefined {
  return getContextValue<string>(SPAN_ID_KEY);
}

/**
 * Set a value in the current async context
 */
export function setContextValue(key: string, value: unknown): void {
  const store = asyncStorage.getStore();
  if (store) {
    store.set(key, value);
  }
}

/**
 * Get all context values as a LoggerContext object
 */
export function getContextAsLoggerContext(): LoggerContext {
  const store = asyncStorage.getStore();
  if (!store) return {};

  const context: LoggerContext = {};
  for (const [key, value] of store.entries()) {
    context[key] = value;
  }
  return context;
}

/**
 * Create a child span within the current context
 * Returns the new span ID
 */
export function createChildSpan(): string {
  const currentSpanId = getSpanId();
  const newSpanId = generateSpanId();

  setContextValue(PARENT_SPAN_ID_KEY, currentSpanId);
  setContextValue(SPAN_ID_KEY, newSpanId);

  return newSpanId;
}

/**
 * Check if currently running within an async context
 */
export function hasContext(): boolean {
  return asyncStorage.getStore() !== undefined;
}
