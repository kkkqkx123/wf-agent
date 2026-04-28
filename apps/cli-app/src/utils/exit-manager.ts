/**
 * Exit Manager
 * Ensure safe exit in headless mode
 */

import { getOutput } from "./output.js";

/**
 * Exit Manager
 * Ensure safe exit in headless mode
 */
export class ExitManager {
  private static isShuttingDown = false;

  /**
   * Safe exit
   */
  static async exit(code: number = 0): Promise<never> {
    if (ExitManager.isShuttingDown) {
      return process.exit(code);
    }

    ExitManager.isShuttingDown = true;
    const output = getOutput();

    try {
      // Wait for all output to complete
      await output.ensureDrained();

      // Wait for event loop to clear
      await ExitManager.waitForEventLoopDrain();

      process.exit(code);
    } catch {
      process.exit(code || 1);
    }
  }

  /**
   * Wait for event loop to clear
   */
  private static async waitForEventLoopDrain(): Promise<void> {
    return new Promise(resolve => {
      // Yield execution using setImmediate
      setImmediate(() => {
        // Check if there are any unfinished async operations
        const checkInterval = setInterval(() => {
          // Simplified check: wait for an event loop cycle
          setImmediate(() => {
            clearInterval(checkInterval);
            resolve();
          });
        }, 10);

        // Timeout protection
        setTimeout(() => {
          clearInterval(checkInterval);
          resolve();
        }, 1000);
      });
    });
  }

  /**
   * Check if currently shutting down
   */
  static get isExiting(): boolean {
    return ExitManager.isShuttingDown;
  }
}

/**
 * Check if running in headless mode
 */
export function isHeadlessMode(): boolean {
  return (
    process.env["CLI_MODE"] === "headless" ||
    process.env["HEADLESS"] === "true" ||
    process.env["TEST_MODE"] === "true"
  );
}

/**
 * Check if running in programmatic mode
 */
export function isProgrammaticMode(): boolean {
  return process.env["CLI_MODE"] === "programmatic";
}

/**
 * Detect execution mode
 */
export function detectExecutionMode(): "interactive" | "headless" | "programmatic" {
  if (process.env["CLI_MODE"] === "programmatic") {
    return "programmatic";
  }
  if (
    process.env["CLI_MODE"] === "headless" ||
    process.env["HEADLESS"] === "true" ||
    process.env["TEST_MODE"] === "true"
  ) {
    return "headless";
  }
  return "interactive";
}
