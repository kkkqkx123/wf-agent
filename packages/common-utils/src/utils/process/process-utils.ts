/**
 * Process utilities for cross-platform compatibility
 */

/**
 * Convert a signal name to its corresponding exit code (Unix convention)
 * 
 * On Unix systems, when a process is terminated by a signal, the exit code
 * is typically 128 + signal_number. This utility helps maintain consistency.
 * 
 * @param signal Signal name (e.g., 'SIGTERM', 'SIGINT')
 * @returns Exit code according to Unix convention
 * 
 * @example
 * ```typescript
 * signalToExitCode('SIGTERM'); // Returns 143 (128 + 15)
 * signalToExitCode('SIGINT');  // Returns 130 (128 + 2)
 * ```
 */
export function signalToExitCode(signal: string): number {
  const signalNumbers: Record<string, number> = {
    SIGHUP: 1,
    SIGINT: 2,
    SIGQUIT: 3,
    SIGILL: 4,
    SIGTRAP: 5,
    SIGABRT: 6,
    SIGBUS: 7,
    SIGFPE: 8,
    SIGKILL: 9,
    SIGUSR1: 10,
    SIGSEGV: 11,
    SIGUSR2: 12,
    SIGPIPE: 13,
    SIGALRM: 14,
    SIGTERM: 15,
  };

  const signalNumber = signalNumbers[signal];
  if (signalNumber === undefined) {
    // Unknown signal, return generic error code
    return 1;
  }

  return 128 + signalNumber;
}

/**
 * Get platform-specific information
 */
export interface PlatformInfo {
  /** Platform name */
  platform: NodeJS.Platform;
  /** Whether running on Windows */
  isWindows: boolean;
  /** Whether running on Unix-like system */
  isUnix: boolean;
  /** Path separator ('\\' on Windows, '/' on Unix) */
  pathSeparator: string;
  /** Line ending ('\\r\\n' on Windows, '\\n' on Unix) */
  lineEnding: string;
}

/**
 * Get current platform information
 */
export function getPlatformInfo(): PlatformInfo {
  const isWindows = process.platform === 'win32';
  
  return {
    platform: process.platform,
    isWindows,
    isUnix: !isWindows,
    pathSeparator: isWindows ? '\\' : '/',
    lineEnding: isWindows ? '\r\n' : '\n',
  };
}

/**
 * Check if running in a production environment
 */
export function isProduction(): boolean {
  return process.env['NODE_ENV'] === 'production';
}

/**
 * Check if running in a development environment
 */
export function isDevelopment(): boolean {
  return process.env['NODE_ENV'] === 'development' || !process.env['NODE_ENV'];
}

/**
 * Check if running in a test environment
 */
export function isTest(): boolean {
  return process.env['NODE_ENV'] === 'test';
}

/**
 * Get the maximum recommended shutdown timeout based on platform
 * 
 * On Windows, when SIGHUP is received (console window close),
 * the OS gives approximately 10 seconds before forced termination.
 * 
 * @param preferredTimeout Preferred timeout in milliseconds
 * @returns Adjusted timeout that respects platform limitations
 */
export function getMaxShutdownTimeout(preferredTimeout: number): number {
  const platformInfo = getPlatformInfo();
  
  if (platformInfo.isWindows) {
    // Windows gives ~10 seconds for cleanup on console close
    // Use a slightly shorter timeout to ensure we finish before OS kills us
    return Math.min(preferredTimeout, 8000);
  }
  
  return preferredTimeout;
}

/**
 * Normalize a path for the current platform
 * 
 * @param path Path to normalize
 * @returns Normalized path
 */
export function normalizePath(path: string): string {
  const platformInfo = getPlatformInfo();
  
  if (platformInfo.isWindows) {
    // Convert forward slashes to backslashes on Windows
    return path.replace(/\//g, '\\');
  } else {
    // Convert backslashes to forward slashes on Unix
    return path.replace(/\\/g, '/');
  }
}
