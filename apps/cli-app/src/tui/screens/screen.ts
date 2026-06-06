/**
 * Screen interface for TUI application
 */

import type { Component } from "../core/tui.js";

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
   * Handle keyboard input at screen level
   * @returns true if input was handled, false otherwise
   */
  handleInput?(data: string): boolean;

  /**
   * Cleanup resources when screen is destroyed
   */
  destroy?(): void;
}
