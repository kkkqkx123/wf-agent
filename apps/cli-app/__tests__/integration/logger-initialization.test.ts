/**
 * Logger initialization order tests
 * Verifies that the lazy logger configuration works correctly
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { 
  createLazyLogger, 
  configureLazyLogger, 
  checkLoggerInitialization,
  clearLazyLoggerCache,
  type Logger 
} from '@wf-agent/common-utils';

describe('Logger Initialization Order', () => {
  beforeEach(() => {
    // Clean up lazy logger state before each test to ensure isolation
    clearLazyLoggerCache();
  });
  it('should warn when configuring an already initialized logger', () => {
    const warnings: string[] = [];
    const originalWarn = console.warn;
    console.warn = (...args) => {
      warnings.push(args.join(' '));
    };

    try {
      // Create a lazy logger
      let initialized = false;
      const factory = () => {
        initialized = true;
        return {
          setLevel: () => {},
          setStream: () => {},
          info: () => {},
          debug: () => {},
          warn: () => {},
          error: () => {},
          child: () => ({} as Logger),
          getLevel: () => 'info' as const,
          isLevelEnabled: () => true,
        } as Logger;
      };

      const logger = createLazyLogger('test-logger', factory);
      
      // Access the logger to trigger initialization
      logger.info('test message');
      expect(initialized).toBe(true);

      // Now try to configure it - should trigger warning
      configureLazyLogger('test-logger', { level: 'debug' });
      
      expect(warnings.length).toBeGreaterThan(0);
      expect(warnings[0]).toContain('test-logger');
      expect(warnings[0]).toContain('already initialized');
    } finally {
      console.warn = originalWarn;
    }
  });

  it('should not warn when configuring before initialization', () => {
    const warnings: string[] = [];
    const originalWarn = console.warn;
    console.warn = (...args) => {
      warnings.push(args.join(' '));
    };

    try {
      // Configure before creating the logger
      configureLazyLogger('test-logger-2', { level: 'debug' });

      // Now create and access the logger
      let initialized = false;
      const factory = () => {
        initialized = true;
        return {
          setLevel: () => {},
          setStream: () => {},
          info: () => {},
          debug: () => {},
          warn: () => {},
          error: () => {},
          child: () => ({} as Logger),
          getLevel: () => 'info' as const,
          isLevelEnabled: () => true,
        } as Logger;
      };

      const logger = createLazyLogger('test-logger-2', factory);
      logger.info('test message');

      // Should not have any warnings
      expect(warnings.length).toBe(0);
    } finally {
      console.warn = originalWarn;
    }
  });

  it('should detect uninitialized loggers without configuration', () => {
    // Use unique name
    const loggerName = 'unconfigured-logger-' + Date.now();
    
    // Create a logger without configuring it first
    const factory = () => {
      return {
        setLevel: () => {},
        setStream: () => {},
        info: () => {},
        debug: () => {},
        warn: () => {},
        error: () => {},
        child: () => ({} as Logger),
        getLevel: () => 'info' as const,
        isLevelEnabled: () => true,
      } as Logger;
    };

    const logger = createLazyLogger(loggerName, factory);
    logger.info('test');

    // Check for initialization issues
    const warnings = checkLoggerInitialization();
    
    // Should detect that this logger was initialized without configuration
    expect(warnings.some(w => w.includes(loggerName))).toBe(true);
  });

  it('should properly apply configuration to lazy loggers', () => {
    // Use a unique logger name to avoid conflicts with other tests
    const loggerName = 'configured-logger-test-' + Date.now();
    
    let capturedLevel: string | undefined;
    
    // Configure first
    configureLazyLogger(loggerName, { level: 'debug' });

    // Then create and use - the factory should receive the configured level
    const factory = () => {
      return {
        setLevel: (level: string) => {
          capturedLevel = level;
        },
        setStream: () => {},
        info: () => {},
        debug: () => {},
        warn: () => {},
        error: () => {},
        child: () => ({} as Logger),
        getLevel: () => 'info' as const,
        isLevelEnabled: () => true,
      } as Logger;
    };

    const logger = createLazyLogger(loggerName, factory);
    logger.info('test');

    // The logger should have been configured with 'debug' level
    expect(capturedLevel).toBe('debug');
  });
});
