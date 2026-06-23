/**
 * Error Converter Tests
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ErrorConverter, KitError, KitErrorCode } from '@/converters/error.converter.js';

describe('ErrorConverter', () => {
  let converter: ErrorConverter;

  beforeEach(() => {
    converter = new ErrorConverter();
  });

  describe('convertResult', () => {
    it('should extract data from success result', () => {
      const result = {
        success: true,
        data: { id: '123', name: 'test' },
      };

      const data = converter.convertResult(result);

      expect(data).toEqual({ id: '123', name: 'test' });
    });

    it('should throw KitError on failure result', () => {
      const result = {
        success: false,
        error: { code: 'WORKFLOW_NOT_FOUND', message: 'Workflow not found' },
      };

      expect(() => converter.convertResult(result)).toThrow(KitError);
    });

    it('should handle non-Result types', () => {
      const data = { id: '123' };

      const result = converter.convertResult(data);

      expect(result).toEqual(data);
    });
  });

  describe('convertError', () => {
    it('should convert SDK error code to KitErrorCode', () => {
      const error = {
        code: 'WORKFLOW_NOT_FOUND',
        message: 'Workflow not found',
      };

      const kitError = converter.convertError(error);

      expect(kitError).toBeInstanceOf(KitError);
      expect(kitError.code).toBe(KitErrorCode.WORKFLOW_NOT_FOUND);
      expect(kitError.message).toBe('Workflow not found');
    });

    it('should handle KitError pass-through', () => {
      const original = new KitError(
        'Test error',
        KitErrorCode.VALIDATION_ERROR
      );

      const result = converter.convertError(original);

      expect(result).toBe(original);
    });

    it('should use INTERNAL_ERROR for unknown codes', () => {
      const error = {
        code: 'UNKNOWN_ERROR',
        message: 'Unknown error',
      };

      const kitError = converter.convertError(error);

      expect(kitError.code).toBe(KitErrorCode.INTERNAL_ERROR);
    });

    it('should handle plain Error objects', () => {
      const error = new Error('Test error');

      const kitError = converter.convertError(error);

      expect(kitError).toBeInstanceOf(KitError);
      expect(kitError.message).toBe('Test error');
    });
  });

  describe('KitError', () => {
    it('should have correct toString format', () => {
      const error = new KitError(
        'Test error',
        KitErrorCode.VALIDATION_ERROR
      );

      expect(error.toString()).toBe('KitError[VALIDATION_ERROR]: Test error');
    });

    it('should store context', () => {
      const context = { field: 'email', value: 'invalid' };
      const error = new KitError(
        'Validation failed',
        KitErrorCode.VALIDATION_ERROR,
        context
      );

      expect(error.context).toEqual(context);
    });
  });
});
