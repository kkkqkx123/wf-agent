/**
 * Event Message Types
 *
 * Custom event messages for extensibility.
 */

/**
 * Event Message Type
 */
export enum EventMessageType {
  /** Event triggered */
  TRIGGER = "event.trigger",

  /** Event processing started */
  PROCESS_START = "event.process_start",

  /** Event processing completed */
  PROCESS_END = "event.process_end",

  /** Custom event */
  CUSTOM = "event.custom",
}

/**
 * Event Trigger Data
 */
export interface EventTriggerData {
  /** Event name */
  eventName: string;

  /** Event source */
  source: string;

  /** Event payload */
  payload: Record<string, unknown>;

  /** Event timestamp */
  eventTimestamp: number;

  /** Whether event is async */
  async: boolean;
}

/**
 * Event Process Start Data
 */
export interface EventProcessStartData {
  /** Event name */
  eventName: string;

  /** Handler name */
  handlerName: string;

  /** Event ID */
  eventId: string;
}

/**
 * Event Process End Data
 */
export interface EventProcessEndData {
  /** Event name */
  eventName: string;

  /** Handler name */
  handlerName: string;

  /** Event ID */
  eventId: string;

  /** Whether successful */
  success: boolean;

  /** Processing duration in milliseconds */
  duration: number;

  /** Error message (if failed) */
  error?: string;
}

/**
 * Event Custom Data
 */
export interface EventCustomData {
  /** Custom event name */
  name: string;

  /** Custom event data */
  data: Record<string, unknown>;

  /** Event source */
  source?: string;

  /** Event tags */
  tags?: string[];
}
