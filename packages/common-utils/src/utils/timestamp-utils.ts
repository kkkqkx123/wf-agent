/**
 * Timestamp Tool Functions
 * Provides timestamp creation, conversion and formatting functions
 */

import type { Timestamp } from "@wf-agent/types";

/**
 * Creating the current timestamp
 */
export function now(): Timestamp {
  return Date.now();
}

/**
 * Creating timestamps from Date
 */
export function timestampFromDate(date: Date): Timestamp {
  return date.getTime();
}

/**
 * Convert to Date object
 */
export function timestampToDate(timestamp: Timestamp): Date {
  return new Date(timestamp);
}

/**
 * Convert to ISO string
 */
export function timestampToISOString(timestamp: Timestamp): string {
  return new Date(timestamp).toISOString();
}

/**
 * Creating timestamps with time zone information
 * @returns Object containing timestamp and time zone information
 */
export function nowWithTimezone(): { timestamp: Timestamp; timezone: string } {
  return {
    timestamp: Date.now(),
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
  };
}

/**
 * Calculate the time difference in milliseconds
 * @param start Start timestamp
 * @param end End timestamp
 * @returns Time difference in milliseconds
 */
export function diffTimestamp(start: Timestamp, end: Timestamp): number {
  return end - start;
}

/**
 * Formatting duration
 * @param ms duration in milliseconds
 * @returns Formatted duration string
 */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`;
  if (ms < 3600000) return `${(ms / 60000).toFixed(2)}min`;
  return `${(ms / 3600000).toFixed(2)}h`;
}
