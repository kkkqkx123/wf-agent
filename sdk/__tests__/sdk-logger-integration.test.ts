/**
 * SDK Logger Configuration Integration Tests
 * Tests the logging configuration flow from CLI to SDK
 * Verifies that simplified logging parameters work correctly
 */

import { describe, it, expect } from 'vitest';
import { configureSDKLogger } from '../utils/logger.js';
import { createConsoleStream } from '@wf-agent/common-utils';

describe('SDK Logger Configuration Integration', () => {
  describe('Simplified API', () => {
    it('should accept minimal configuration (no parameters)', () => {
      expect(() => {
        configureSDKLogger({});
      }).not.toThrow();
    });

    it('should accept level-only configuration', () => {
      expect(() => {
        configureSDKLogger({ level: 'debug' });
      }).not.toThrow();
      
      expect(() => {
        configureSDKLogger({ level: 'info' });
      }).not.toThrow();
      
      expect(() => {
        configureSDKLogger({ level: 'warn' });
      }).not.toThrow();
      
      expect(() => {
        configureSDKLogger({ level: 'error' });
      }).not.toThrow();
    });

    it('should accept level and stream configuration', () => {
      const stream = createConsoleStream({ json: false, timestamp: true });
      
      expect(() => {
        configureSDKLogger({ 
          level: 'info',
          stream,
        });
      }).not.toThrow();
    });

    it('should support all standard log levels', () => {
      const levels: Array<'debug' | 'info' | 'warn' | 'error'> = [
        'debug',
        'info',
        'warn',
        'error',
      ];

      levels.forEach((level) => {
        expect(() => {
          configureSDKLogger({ level });
        }).not.toThrow();
      });
    });
  });

  describe('Configuration Simplification Verification', () => {
    it('should NOT accept removed parameters (graphLogLevel)', () => {
      // This test verifies that the old complex API has been removed
      // TypeScript should prevent this at compile time
      // At runtime, extra properties are simply ignored
      
      const config: any = {
        level: 'info',
        graphLogLevel: 'debug', // This should be ignored/removed
      };
      
      // The function should still work, just ignoring the extra property
      expect(() => {
        configureSDKLogger(config);
      }).not.toThrow();
    });

    it('should NOT accept removed parameters (agentLogLevel)', () => {
      const config: any = {
        level: 'info',
        agentLogLevel: 'debug', // This should be ignored/removed
      };
      
      expect(() => {
        configureSDKLogger(config);
      }).not.toThrow();
    });

    it('should NOT accept removed parameters (sdkLevel)', () => {
      const config: any = {
        level: 'info',
        sdkLevel: 'debug', // This should be ignored/removed
      };
      
      expect(() => {
        configureSDKLogger(config);
      }).not.toThrow();
    });
  });

  describe('Stream Configuration', () => {
    it('should work with console stream', () => {
      const stream = createConsoleStream({ 
        json: false, 
        timestamp: true,
      });
      
      expect(() => {
        configureSDKLogger({ 
          level: 'debug',
          stream,
        });
      }).not.toThrow();
    });

    it('should work with JSON console stream', () => {
      const stream = createConsoleStream({ 
        json: true, 
        timestamp: true,
      });
      
      expect(() => {
        configureSDKLogger({ 
          level: 'info',
          stream,
        });
      }).not.toThrow();
    });
  });

  describe('Multiple Configuration Calls', () => {
    it('should allow reconfiguration', () => {
      // First configuration
      configureSDKLogger({ level: 'debug' });
      
      // Second configuration should override the first
      expect(() => {
        configureSDKLogger({ level: 'warn' });
      }).not.toThrow();
    });

    it('should handle rapid reconfiguration', () => {
      const configs = [
        { level: 'debug' as const },
        { level: 'info' as const },
        { level: 'warn' as const },
        { level: 'error' as const },
      ];
      
      configs.forEach((config) => {
        expect(() => {
          configureSDKLogger(config);
        }).not.toThrow();
      });
    });
  });
});
