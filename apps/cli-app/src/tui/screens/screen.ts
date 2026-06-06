/**
 * Screen interface for TUI application
 */

import type { Component } from "../core/tui.js";
import type { BaseComponentMessage } from "@wf-agent/types";

/**
 * Interface that all screens must implement
 */
export interface Screen {
  /**
   * Render the screen content
   */
  render(): Component;

  /**
   * Called when screen becomes active
   */
  onActivate?(): void;

  /**
   * Called when screen becomes inactive
   */
  onDeactivate?(): void;

  /**
   * Called when a component message is routed to this screen.
   * Screens should implement this instead of subscribing to MessageBus directly.
   */
  onMessage?(message: BaseComponentMessage): void;

  /**
   * Handle keyboard input at screen level
   * @returns true if input was handled, false otherwise
   */
  handleInput?(data: string): boolean;

  /**
   * Cleanup resources when screen is destroyed
   */
  destroy?(): void;
}
