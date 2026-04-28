/**
 * Human Relay Message Types
 *
 * Messages related to human relay (human intervention in agent execution).
 */

/**
 * Human Relay Message Type
 */
export enum HumanRelayMessageType {
  /** Human relay request */
  REQUEST = "human_relay.request",

  /** Human relay response */
  RESPONSE = "human_relay.response",

  /** Human relay timeout */
  TIMEOUT = "human_relay.timeout",

  /** Human relay cancelled */
  CANCEL = "human_relay.cancel",
}

/**
 * Human Relay Request Data
 */
export interface HumanRelayRequestData {
  /** Request ID */
  requestId: string;

  /** Full prompt for human */
  prompt: string;

  /** Conversation context */
  context?: {
    /** Recent messages */
    messages?: Array<{
      role: "user" | "assistant" | "system";
      content: string;
      timestamp: number;
    }>;

    /** Additional metadata */
    metadata?: Record<string, unknown>;
  };

  /** Timeout in milliseconds */
  timeout: number;

  /** Output file path (for CLI) */
  outputFile?: string;

  /** Input file path (for CLI) */
  inputFile?: string;

  /** Instructions for human */
  instructions?: string;
}

/**
 * Human Relay Response Data
 */
export interface HumanRelayResponseData {
  /** Request ID */
  requestId: string;

  /** Human response content */
  content: string;

  /** Response time in milliseconds */
  responseTime: number;

  /** Response source */
  source: "file" | "api" | "ui";
}

/**
 * Human Relay Timeout Data
 */
export interface HumanRelayTimeoutData {
  /** Request ID */
  requestId: string;

  /** Timeout duration in milliseconds */
  timeout: number;

  /** Whether auto-continue is enabled */
  autoContinue?: boolean;
}

/**
 * Human Relay Cancel Data
 */
export interface HumanRelayCancelData {
  /** Request ID */
  requestId: string;

  /** Cancel reason */
  reason: "user_cancel" | "error" | "shutdown";

  /** Cancel message */
  message?: string;
}
