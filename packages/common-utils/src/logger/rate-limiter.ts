/**
 * Log Rate Limiter
 * Provides rate limiting functionality to prevent log flooding
 */

/**
 * Rate limiter for log throttling
 */
export class LogRateLimiter {
  private maxLogsPerSecond: number;
  private logCount: number = 0;
  private lastResetTime: number = Date.now();
  private droppedCount: number = 0;

  constructor(maxLogsPerSecond: number) {
    this.maxLogsPerSecond = maxLogsPerSecond;
  }

  /**
   * Check if a log should be allowed
   * Returns true if allowed, false if rate limited
   */
  allow(): boolean {
    const now = Date.now();
    if (now - this.lastResetTime >= 1000) {
      // Reset counter every second
      if (this.droppedCount > 0) {
        // Log how many were dropped (once per second) using stderr directly to avoid recursion
        process.stderr.write(
          `[Logger] Rate limit exceeded, dropped ${this.droppedCount} logs in last second\n`,
        );
        this.droppedCount = 0;
      }
      this.logCount = 0;
      this.lastResetTime = now;
    }

    if (this.logCount < this.maxLogsPerSecond) {
      this.logCount++;
      return true;
    }

    this.droppedCount++;
    return false;
  }
}
