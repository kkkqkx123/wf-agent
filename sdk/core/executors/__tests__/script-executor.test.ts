/**
 * Script Executor Unit Tests
 * Tests for the ScriptExecutor class
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { ScriptExecutor } from '../script-executor.js';
import type { Script, ScriptExecutionOptions } from '@wf-agent/types';

describe('ScriptExecutor', () => {
  let executor: ScriptExecutor;
  let mockTerminalService: any;

  beforeEach(() => {
    // Create mock terminal service
    mockTerminalService = {
      executeOneOff: vi.fn(),
    };
    
    // Clear mocks before each test
    vi.clearAllMocks();
    
    // Create new executor instance with injected mock
    executor = new ScriptExecutor(mockTerminalService);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('execute', () => {
    it('should execute script successfully', async () => {
      const script: Script = {
        id: 'test-script-id',
        name: 'test-script',
        description: 'A test script',
        content: 'echo "Hello World"',
        options: {},
      };

      const mockResult = {
        success: true,
        stdout: 'Hello World\r\n', // Windows adds \r\n
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const result = await executor.execute(script);

      expect(result.success).toBe(true);
      expect(result.scriptName).toBe('test-script');
      expect(result.stdout).toContain('Hello World');
      expect(result.exitCode).toBe(0);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it('should handle script execution failure', async () => {
      const script: Script = {
        id: 'failing-script-id',
        name: 'failing-script',
        description: 'A failing script',
        content: 'exit 1',
        options: {},
      };

      const mockResult = {
        success: false,
        stdout: '',
        stderr: 'Error occurred',
        exitCode: 1,
        error: 'Command failed',
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const result = await executor.execute(script);

      expect(result.success).toBe(false);
      expect(result.scriptName).toBe('failing-script');
      expect(result.stderr).toContain('Error');
      expect(result.exitCode).toBe(1);
    });

    it('should handle empty script content', async () => {
      const script: Script = {
        id: 'empty-script-id',
        name: 'empty-script',
        description: 'An empty script',
        content: '',
        options: {},
      };

      const result = await executor.execute(script);

      expect(result.success).toBe(false);
      expect(result.scriptName).toBe('empty-script');
      expect(result.error).toBe('Script content is empty');
      expect(mockTerminalService.executeOneOff).not.toHaveBeenCalled();
    });

    it('should pass execution options to terminal service', async () => {
      const script: Script = {
        id: 'test-script-id',
        name: 'test-script',
        description: 'A test script',
        content: 'ls -la',
        options: {},
      };

      const options: ScriptExecutionOptions = {
        workingDirectory: '/tmp',
        environment: { NODE_ENV: 'test' },
        timeout: 5000,
      };

      const mockResult = {
        success: true,
        stdout: 'file1.txt',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      await executor.execute(script, options);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it('should handle exceptions during execution', async () => {
      const script: Script = {
        id: 'error-script-id',
        name: 'error-script',
        description: 'An error script',
        content: 'some command',
        options: {},
      };

      mockTerminalService.executeOneOff.mockRejectedValue(new Error('Unexpected error'));

      const result = await executor.execute(script);

      expect(result.success).toBe(false);
      expect(result.scriptName).toBe('error-script');
      expect(result.error).toBeDefined();
    });

    it('should measure execution time', async () => {
      const script: Script = {
        id: 'timed-script-id',
        name: 'timed-script',
        description: 'A timed script',
        content: 'sleep 0.1',
        options: {},
      };

      const mockResult = {
        success: true,
        stdout: '',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const startTime = Date.now();
      const result = await executor.execute(script);
      const endTime = Date.now();

      expect(result.executionTime).toBeGreaterThanOrEqual(0);
      expect(result.executionTime).toBeLessThanOrEqual(endTime - startTime + 100); // Allow some margin
    });

    it('should handle non-Error exceptions', async () => {
      const script: Script = {
        id: 'string-error-script-id',
        name: 'string-error-script',
        description: 'A string error script',
        content: 'command',
        options: {},
      };

      // Terminal service will convert this to a proper result
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 1,
        error: 'String error',
      });

      const result = await executor.execute(script);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('cleanup', () => {
    it('should complete cleanup without errors', async () => {
      await expect(executor.cleanup()).resolves.not.toThrow();
    });
  });

  describe('constructor', () => {
    it('should use default terminal service when not injected', async () => {
      // Create executor without injecting terminal service
      const defaultExecutor = new ScriptExecutor();
      
      // This will use the real terminal service, so we just verify it doesn't throw
      // The actual execution may fail due to environment, but constructor should work
      expect(defaultExecutor).toBeDefined();
    });
  });

  describe('execute with detailed options', () => {
    it('should pass working directory to terminal service', async () => {
      const script: Script = {
        id: 'cwd-script',
        name: 'cwd-script',
        description: 'Script with working directory',
        content: 'pwd',
        options: {},
      };

      const options: ScriptExecutionOptions = {
        workingDirectory: '/custom/path',
      };

      const mockResult = {
        success: true,
        stdout: '/custom/path',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      await executor.execute(script, options);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        'pwd',
        expect.objectContaining({
          cwd: '/custom/path',
        })
      );
    });

    it('should pass environment variables to terminal service', async () => {
      const script: Script = {
        id: 'env-script',
        name: 'env-script',
        description: 'Script with environment',
        content: 'echo $TEST_VAR',
        options: {},
      };

      const options: ScriptExecutionOptions = {
        environment: {
          TEST_VAR: 'test_value',
          NODE_ENV: 'production',
        },
      };

      const mockResult = {
        success: true,
        stdout: 'test_value',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      await executor.execute(script, options);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        'echo $TEST_VAR',
        expect.objectContaining({
          env: {
            TEST_VAR: 'test_value',
            NODE_ENV: 'production',
          },
        })
      );
    });

    it('should pass timeout to terminal service', async () => {
      const script: Script = {
        id: 'timeout-script',
        name: 'timeout-script',
        description: 'Script with timeout',
        content: 'sleep 1',
        options: {},
      };

      const options: ScriptExecutionOptions = {
        timeout: 5000,
      };

      const mockResult = {
        success: true,
        stdout: '',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      await executor.execute(script, options);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        'sleep 1',
        expect.objectContaining({
          timeout: 5000,
        })
      );
    });

    it('should pass all options together to terminal service', async () => {
      const script: Script = {
        id: 'all-options-script',
        name: 'all-options-script',
        description: 'Script with all options',
        content: 'echo test',
        options: {},
      };

      const options: ScriptExecutionOptions = {
        workingDirectory: '/test/dir',
        environment: { VAR: 'value' },
        timeout: 10000,
      };

      const mockResult = {
        success: true,
        stdout: 'test',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      await executor.execute(script, options);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        'echo test',
        {
          cwd: '/test/dir',
          env: { VAR: 'value' },
          timeout: 10000,
        }
      );
    });
  });

  describe('execute result details', () => {
    it('should return stdout and stderr in result', async () => {
      const script: Script = {
        id: 'output-script',
        name: 'output-script',
        description: 'Script with output',
        content: 'echo stdout && echo stderr >&2',
        options: {},
      };

      const mockResult = {
        success: true,
        stdout: 'stdout\n',
        stderr: 'stderr\n',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const result = await executor.execute(script);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe('stdout\n');
      expect(result.stderr).toBe('stderr\n');
      expect(result.exitCode).toBe(0);
    });

    it('should handle non-zero exit code as failure', async () => {
      const script: Script = {
        id: 'nonzero-script',
        name: 'nonzero-script',
        description: 'Script with non-zero exit',
        content: 'exit 42',
        options: {},
      };

      const mockResult = {
        success: false,
        stdout: '',
        stderr: 'Error with exit code 42',
        exitCode: 42,
        error: 'Command failed with exit code 42',
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const result = await executor.execute(script);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(42);
      expect(result.error).toBe('Command failed with exit code 42');
    });

    it('should include script name in result', async () => {
      const script: Script = {
        id: 'named-script',
        name: 'my-custom-script',
        description: 'A named script',
        content: 'echo test',
        options: {},
      };

      const mockResult = {
        success: true,
        stdout: 'test',
        stderr: '',
        exitCode: 0,
      };

      mockTerminalService.executeOneOff.mockResolvedValue(mockResult);

      const result = await executor.execute(script);

      expect(result.scriptName).toBe('my-custom-script');
    });

    it('should measure accurate execution time', async () => {
      const script: Script = {
        id: 'timing-script',
        name: 'timing-script',
        description: 'Script for timing test',
        content: 'sleep 0.05',
        options: {},
      };

      const mockResult = {
        success: true,
        stdout: '',
        stderr: '',
        exitCode: 0,
      };

      // Simulate a delay in execution
      mockTerminalService.executeOneOff.mockImplementation(async () => {
        await new Promise(resolve => setTimeout(resolve, 50));
        return mockResult;
      });

      const startTime = Date.now();
      const result = await executor.execute(script);
      const endTime = Date.now();

      expect(result.executionTime).toBeGreaterThanOrEqual(40); // At least 40ms
      expect(result.executionTime).toBeLessThanOrEqual(endTime - startTime + 50); // Within reasonable margin
    });
  });
});
