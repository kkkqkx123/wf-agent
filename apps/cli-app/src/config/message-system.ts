/**
 * CLI-App Message System Setup
 *
 * Initializes the component message system for CLI-App.
 */

import { MessageBus } from "@wf-agent/sdk";
import { CLI_ROUTING_RULES } from "./routing-rules.js";

/**
 * Setup the message system for CLI-App
 * @returns Configured message bus instance
 */
export function setupMessageSystem(): MessageBus {
  // Create message bus with CLI-specific routing rules
  const bus = new MessageBus(CLI_ROUTING_RULES, {
    maxHistorySize: 1000,
    enableHistory: true,
    asyncHandlers: true,
  });

  // Note: TUI and file handlers should be registered by the application
  // when those components are initialized

  return bus;
}
