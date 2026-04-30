/**
 * CLI application-specific type definitions
 */

/**
 * Error code constants
 */
export const ErrorCode = {
  UNKNOWN: "UNKNOWN_ERROR",
  VALIDATION: "VALIDATION_ERROR",
  FILE_OPERATION: "FILE_ERROR",
  API: "API_ERROR",
  NETWORK: "NETWORK_ERROR",
  TIMEOUT: "TIMEOUT_ERROR",
  ADAPTER: "ADAPTER_ERROR",
  CONFIGURATION: "CONFIGURATION_ERROR",
  NOT_FOUND: "NOT_FOUND_ERROR",
  PERMISSION: "PERMISSION_ERROR",
} as const;

/**
 * Error code type
 */
export type ErrorCodeType = (typeof ErrorCode)[keyof typeof ErrorCode];

/**
 * Command options interface
 */
export interface CommandOptions {
  verbose?: boolean;
  debug?: boolean;
  output?: "json" | "table" | "plain";
  table?: boolean;
  params?: string;
}

/**
 * Workflow command options
 */
export interface WorkflowCommandOptions extends CommandOptions {
  name?: string;
  tags?: string[];
  version?: string;
}

/**
 * WorkflowExecution command options
 */
export interface WorkflowExecutionCommandOptions extends CommandOptions {
  input?: string;
  detached?: boolean;
  timeout?: number;
}

/**
 * Checkpoint command options
 */
export interface CheckpointCommandOptions extends CommandOptions {
  name?: string;
  description?: string;
}

/**
 * Template command options
 */
export interface TemplateCommandOptions extends CommandOptions {
  type?: "node" | "workflow" | "trigger";
  category?: string;
}

/**
 * File parse result
 */
export interface FileParseResult<T = unknown> {
  content: T;
  format: "json" | "yaml" | "toml";
  path: string;
}

/**
 * CLI base error class
 * Base class for all CLI errors, providing unified error handling interface
 */
export class CLIError extends Error {
  constructor(
    message: string,
    public code: ErrorCodeType = ErrorCode.UNKNOWN,
    public exitCode: number = 1,
  ) {
    super(message);
    this.name = "CLIError";
    Object.setPrototypeOf(this, CLIError.prototype);
  }
}

/**
 * Validation error class
 * Used for input validation failure scenarios
 */
export class CLIValidationError extends CLIError {
  constructor(
    message: string,
    public field?: string,
  ) {
    super(message, ErrorCode.VALIDATION, 2);
    this.name = "CLIValidationError";
    Object.setPrototypeOf(this, CLIValidationError.prototype);
  }
}

/**
 * File operation error class
 * Used for file read/write, parsing and other operation failure scenarios
 */
export class CLIFileOperationError extends CLIError {
  constructor(
    message: string,
    public filePath?: string,
  ) {
    super(message, ErrorCode.FILE_OPERATION, 3);
    this.name = "CLIFileOperationError";
    Object.setPrototypeOf(this, CLIFileOperationError.prototype);
  }
}

/**
 * API error class
 * Used for API call failure scenarios
 */
export class CLIAPIError extends CLIError {
  constructor(
    message: string,
    public statusCode?: number,
    public apiEndpoint?: string,
  ) {
    super(message, ErrorCode.API, 4);
    this.name = "CLIAPIError";
    Object.setPrototypeOf(this, CLIAPIError.prototype);
  }
}

/**
 * Network error class
 * Used for network connection failure, DNS resolution failure and other scenarios
 */
export class CLINetworkError extends CLIError {
  constructor(
    message: string,
    public url?: string,
  ) {
    super(message, ErrorCode.NETWORK, 5);
    this.name = "CLINetworkError";
    Object.setPrototypeOf(this, CLINetworkError.prototype);
  }
}

/**
 * Timeout error class
 * Used for operation timeout scenarios
 */
export class CLITimeoutError extends CLIError {
  constructor(
    message: string,
    public timeout?: number,
  ) {
    super(message, ErrorCode.TIMEOUT, 6);
    this.name = "CLITimeoutError";
    Object.setPrototypeOf(this, CLITimeoutError.prototype);
  }
}

/**
 * Not found error class
 * Used for resource not found scenarios
 */
export class CLINotFoundError extends CLIError {
  constructor(
    message: string,
    public resourceType?: string,
    public resourceId?: string,
  ) {
    super(message, ErrorCode.NOT_FOUND, 7);
    this.name = "CLINotFoundError";
    Object.setPrototypeOf(this, CLINotFoundError.prototype);
  }
}

/**
 * Permission error class
 * Used for insufficient permissions scenarios
 */
export class CLIPermissionError extends CLIError {
  constructor(
    message: string,
    public requiredPermission?: string,
  ) {
    super(message, ErrorCode.PERMISSION, 8);
    this.name = "CLIPermissionError";
    Object.setPrototypeOf(this, CLIPermissionError.prototype);
  }
}

/**
 * Configuration error class
 * Used for configuration loading, parsing failure scenarios
 */
export class CLIConfigurationError extends CLIError {
  constructor(
    message: string,
    public configPath?: string,
  ) {
    super(message, ErrorCode.CONFIGURATION, 9);
    this.name = "CLIConfigurationError";
    Object.setPrototypeOf(this, CLIConfigurationError.prototype);
  }
}
