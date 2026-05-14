/**
 * Circuit Breaker Tests
 */

import { describe, it, expect } from 'vitest';
import { CircuitBreaker } from '../circuit-breaker.js';

describe('Circuit Breaker', () => {
  describe('Basic Functionality', () => {
    it('should start in CLOSED state', () => {
      const breaker = new CircuitBreaker();
      expect(breaker.getState()).toBe('CLOSED');
    });

    it('should allow requests when closed', async () => {
      const breaker = new CircuitBreaker();
      const fn = async () => 'success';
      
      const result = await breaker.execute(fn);
      expect(result).toBe('success');
    });

    it('should open after reaching failure threshold', async () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 3,
        resetTimeout: 1000,
      });

      const failingFn = async () => {
        throw new Error('Test error');
      };

      // Fail 3 times
      for (let i = 0; i < 3; i++) {
        await expect(breaker.execute(failingFn)).rejects.toThrow('Test error');
      }

      expect(breaker.getState()).toBe('OPEN');
    });

    it('should reject requests when open', async () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 2,
        resetTimeout: 5000,
      });

      // Open the circuit
      const failingFn = async () => {
        throw new Error('Error');
      };

      await expect(breaker.execute(failingFn)).rejects.toThrow();
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      // Should be open now
      expect(breaker.isOpen()).toBe(true);

      // Next request should fail immediately
      const successFn = async () => 'success';
      await expect(breaker.execute(successFn)).rejects.toThrow('Circuit breaker is OPEN');
    });

    it('should transition to HALF_OPEN after timeout', async () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 2,
        resetTimeout: 100, // 100ms for fast test
      });

      // Open the circuit
      const failingFn = async () => {
        throw new Error('Error');
      };

      await expect(breaker.execute(failingFn)).rejects.toThrow();
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      expect(breaker.getState()).toBe('OPEN');

      // Wait for reset timeout
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should transition to HALF_OPEN on next check
      expect(breaker.isOpen()).toBe(false);
      expect(breaker.getState()).toBe('HALF_OPEN');
    });

    it('should close after successful requests in HALF_OPEN', async () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 2,
        successThreshold: 2,
        resetTimeout: 100,
      });

      // Open the circuit
      const failingFn = async () => {
        throw new Error('Error');
      };

      await expect(breaker.execute(failingFn)).rejects.toThrow();
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      // Wait for reset
      await new Promise(resolve => setTimeout(resolve, 150));

      // Successful requests should close the circuit
      const successFn = async () => 'success';
      await breaker.execute(successFn);
      await breaker.execute(successFn);

      expect(breaker.getState()).toBe('CLOSED');
    });

    it('should reopen on failure in HALF_OPEN', async () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 2,
        resetTimeout: 100,
      });

      // Open the circuit
      const failingFn = async () => {
        throw new Error('Error');
      };

      await expect(breaker.execute(failingFn)).rejects.toThrow();
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      // Wait for reset
      await new Promise(resolve => setTimeout(resolve, 150));

      // Failure in HALF_OPEN should reopen
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      expect(breaker.getState()).toBe('OPEN');
    });
  });

  describe('Manual Control', () => {
    it('should allow manual reset', () => {
      const breaker = new CircuitBreaker({
        failureThreshold: 2,
      });

      // Force open
      breaker.forceOpen();
      expect(breaker.getState()).toBe('OPEN');

      // Manual reset
      breaker.reset();
      expect(breaker.getState()).toBe('CLOSED');
    });

    it('should allow forcing open', () => {
      const breaker = new CircuitBreaker();
      breaker.forceOpen();
      expect(breaker.getState()).toBe('OPEN');
    });
  });

  describe('Metrics', () => {
    it('should track metrics', async () => {
      const breaker = new CircuitBreaker();

      const successFn = async () => 'success';
      const failingFn = async () => {
        throw new Error('Error');
      };

      await breaker.execute(successFn);
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      const metrics = breaker.getMetrics();
      expect(metrics.totalRequests).toBe(2);
      expect(metrics.totalSuccesses).toBe(1);
      expect(metrics.totalFailures).toBe(1);
    });
  });

  describe('Monitor Callback', () => {
    it('should call monitor on state changes', async () => {
      const monitorCalls: Array<{ state: string }> = [];
      const monitor = (state: any) => {
        monitorCalls.push({ state });
      };

      const breaker = new CircuitBreaker({
        failureThreshold: 2,
        monitor,
      });

      const failingFn = async () => {
        throw new Error('Error');
      };

      await expect(breaker.execute(failingFn)).rejects.toThrow();
      await expect(breaker.execute(failingFn)).rejects.toThrow();

      // Monitor should have been called
      expect(monitorCalls.length).toBeGreaterThan(0);
    });
  });
});
