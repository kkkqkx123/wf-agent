/**
 * CLI Error Handler
 * Provides unified error handling and user-friendly error message display
 */

import { getOutput } from "./output.js";
import { CLIValidationError as ValidationError } from "../types/cli-types.js";
import type { CLIError, CLIAPIError, CLIFileOperationError } from "../types/cli-types.js";

const output = getOutput();

/**
 * Error type enumeration
 */
export enum ErrorType {
  VALIDATION = "VALIDATION_ERROR",
  FILE_OPERATION = "FILE_ERROR",
  API = "API_ERROR",
  NETWORK = "NETWORK_ERROR",
  TIMEOUT = "TIMEOUT_ERROR",
  UNKNOWN = "UNKNOWN_ERROR",
}

/**
 * Error severity level
 */
export enum ErrorSeverity {
  LOW = "low",
  MEDIUM = "medium",
  HIGH = "high",
  CRITICAL = "critical",
}

/**
 * Error context information
 */
export interface ErrorContext {
  command?: string;
  operation?: string;
  filePath?: string;
  url?: string;
  statusCode?: number;
  additionalInfo?: Record<string, any>;
}

/**
 * Formatted error message
 */
export interface FormattedError {
  message: string;
  type: ErrorType;
  severity: ErrorSeverity;
  exitCode: number;
  suggestions?: string[];
}

/**
 * CLI Error Handler Class
 */
export class CLIErrorHandler {
  private verbose: boolean;
  private debug: boolean;

  constructor(options: { verbose?: boolean; debug?: boolean } = {}) {
    this.verbose = options.verbose || false;
    this.debug = options.debug || false;
  }

  /**
   * Handle error and exit program
   * @param error Error object
   * @param context Error context
   */
  handleError(error: unknown, context: ErrorContext = {}): never {
    const formattedError = this.formatError(error, context);
    this.displayError(formattedError, context);
    process.exit(formattedError.exitCode);
  }

  /**
   * Format error message
   * @param error Error object
   * @param context Error context
   * @returns Formatted error message
   */
  formatError(error: unknown, context: ErrorContext = {}): FormattedError {
    let message: string;
    let type: ErrorType;
    let severity: ErrorSeverity;
    let exitCode: number;
    let suggestions: string[] = [];

    if (error instanceof ValidationError) {
      message = error.message;
      type = ErrorType.VALIDATION;
      severity = ErrorSeverity.LOW;
      exitCode = 2;
      suggestions = this.getValidationSuggestions(error);
    } else if (this.isAPIError(error)) {
      message = error.message;
      type = ErrorType.API;
      severity = this.getAPIErrorSeverity(error.statusCode);
      exitCode = 4;
      suggestions = this.getAPISuggestions(error);
    } else if (this.isFileOperationError(error)) {
      message = error.message;
      type = ErrorType.FILE_OPERATION;
      severity = ErrorSeverity.MEDIUM;
      exitCode = 3;
      suggestions = this.getFileOperationSuggestions(error);
    } else if (this.isCLIError(error)) {
      message = error.message;
      type = (ErrorType[error.code as keyof typeof ErrorType] as ErrorType) || ErrorType.UNKNOWN;
      severity = ErrorSeverity.MEDIUM;
      exitCode = error.exitCode;
    } else if (error instanceof Error) {
      message = error.message;
      type = this.inferErrorType(error);
      severity = ErrorSeverity.MEDIUM;
      exitCode = 1;
      suggestions = this.getGenericSuggestions(error);
    } else {
      message = String(error);
      type = ErrorType.UNKNOWN;
      severity = ErrorSeverity.MEDIUM;
      exitCode = 1;
    }

    // Add contextual information
    if (context.operation) {
      message = `${context.operation}: ${message}`;
    }

    return { message, type, severity, exitCode, suggestions };
  }

  /**
   * Display error message
   * @param formattedError Formatted error message
   * @param context Error context
   */
  private displayError(formattedError: FormattedError, context: ErrorContext): void {
    // Use output.error() to output error to stderr
    output.error(formattedError.message);

    // Display the error type
    if (this.verbose) {
      output.error(`Error type: ${formattedError.type}`);
      output.error(`Severity: ${formattedError.severity}`);
    }

    // Display suggestions
    if (formattedError.suggestions && formattedError.suggestions.length > 0) {
      output.newLine();
      output.subsection("Suggestion:");
      formattedError.suggestions.forEach((suggestion, index) => {
        output.output(`  ${index + 1}. ${suggestion}`);
      });
    }

    // Display the stack trace (debug mode).
    if (this.debug && this.getErrorObject(context)) {
      const error = this.getErrorObject(context);
      if (error instanceof Error && error.stack) {
        output.newLine();
        output.subsection("Stack trace:");
        output.output(error.stack);
      }
    }

    // Display context information (detailed mode)
    if (this.verbose && Object.keys(context).length > 0) {
      output.newLine();
      output.subsection("Context Information:");
      Object.entries(context).forEach(([key, value]) => {
        output.keyValue(`  ${key}`, String(value));
      });
    }
  }

  /**
   * Get suggestions for validation errors
   */
  private getValidationSuggestions(error: ValidationError): string[] {
    const suggestions: string[] = [];

    if (error.field) {
      suggestions.push(`Check if the value of field "${error.field}" is correct`);
    }

    if (error.message.includes("File path")) {
      suggestions.push("Ensure the file path is correct and the file exists.");
      suggestions.push("Supported file formats: .json, .toml, .yaml, .yml");
    }

    if (error.message.includes("JSON")) {
      suggestions.push("Ensure the JSON format is correct.");
      suggestions.push("You can use online JSON validation tools to check the format.");
    }

    if (error.message.includes("UUID")) {
      suggestions.push("Ensure that the ID format is a valid UUID.");
    }

    return suggestions;
  }

  /**
   * Get suggestions for API errors
   */
  private getAPISuggestions(error: CLIAPIError): string[] {
    const suggestions: string[] = [];

    if (error.statusCode === 401) {
      suggestions.push("Check if the API key is correct.");
      suggestions.push("Make sure the API key has not expired.");
    } else if (error.statusCode === 403) {
      suggestions.push("Check if there are sufficient permissions to perform this operation.");
    } else if (error.statusCode === 404) {
      suggestions.push("Check if the requested resource exists.");
      suggestions.push("Verify if the resource ID is correct.");
    } else if (error.statusCode === 429) {
      suggestions.push("The request is too frequent; please try again later.");
    } else if (error.statusCode && error.statusCode >= 500) {
      suggestions.push("Server error, please try again later.");
      suggestions.push("If the issue persists, please contact technical support.");
    }

    if (error.apiEndpoint) {
      suggestions.push(`Requested API endpoint: ${error.apiEndpoint}`);
    }

    return suggestions;
  }

  /**
   * Get suggestions for file operation errors
   */
  private getFileOperationSuggestions(error: CLIFileOperationError): string[] {
    const suggestions: string[] = [];

    if (error.filePath) {
      suggestions.push(`Check file path: ${error.filePath}`);
      suggestions.push("Ensure the file exists and is accessible.");
      suggestions.push("Check file permissions.");
    }

    if (error.message.includes("Permissions")) {
      suggestions.push("Ensure there are sufficient file system permissions.");
    }

    if (error.message.includes("Disk space")) {
      suggestions.push("Check if there is sufficient disk space available.");
    }

    return suggestions;
  }

  /**
   * Get suggestions for generic errors
   */
  private getGenericSuggestions(error: Error): string[] {
    const suggestions: string[] = [];

    if (error.message.includes("network")) {
      suggestions.push("Check the network connection.");
      suggestions.push("Ensure that access to the server is possible.");
    }

    if (error.message.includes("Timeout")) {
      suggestions.push("Operation timed out, please try again later.");
      suggestions.push("You can try increasing the timeout period.");
    }

    suggestions.push("Use the --verbose option to obtain more detailed information.");
    suggestions.push("Use the --debug option to view the complete stack trace.");

    return suggestions;
  }

  /**
   * Infer error type
   */
  private inferErrorType(error: Error): ErrorType {
    const message = error.message.toLowerCase();

    if (message.includes("network") || message.includes("Connect")) {
      return ErrorType.NETWORK;
    }

    if (message.includes("timeout") || message.includes("Timeout")) {
      return ErrorType.TIMEOUT;
    }

    if (message.includes("file") || message.includes("file")) {
      return ErrorType.FILE_OPERATION;
    }

    if (message.includes("api") || message.includes("http")) {
      return ErrorType.API;
    }

    return ErrorType.UNKNOWN;
  }

  /**
   * Get API error severity
   */
  private getAPIErrorSeverity(statusCode?: number): ErrorSeverity {
    if (!statusCode) return ErrorSeverity.MEDIUM;

    if (statusCode >= 500) return ErrorSeverity.HIGH;
    if (statusCode >= 400) return ErrorSeverity.MEDIUM;
    return ErrorSeverity.LOW;
  }

  /**
   * Check if it is an API error
   */
  private isAPIError(error: unknown): error is CLIAPIError {
    return (
      typeof error === "object" && error !== null && "name" in error && error.name === "CLIAPIError"
    );
  }

  /**
   * Check if it is a file operation error
   */
  private isFileOperationError(error: unknown): error is CLIFileOperationError {
    return (
      typeof error === "object" &&
      error !== null &&
      "name" in error &&
      error.name === "CLIFileOperationError"
    );
  }

  /**
   * Check if it is a CLI error
   */
  private isCLIError(error: unknown): error is CLIError {
    return (
      typeof error === "object" && error !== null && "name" in error && error.name === "CLIError"
    );
  }

  /**
   * Get error object
   */
  private getErrorObject(context: ErrorContext): Error | null {
    if (context.additionalInfo?.["error"] instanceof Error) {
      return context.additionalInfo["error"];
    }
    return null;
  }

  /**
   * Set verbose mode
   */
  setVerbose(verbose: boolean): void {
    this.verbose = verbose;
  }

  /**
   * Set debug mode
   */
  setDebug(debug: boolean): void {
    this.debug = debug;
  }
}

/**
 * Global error handler instance
 */
let globalErrorHandler: CLIErrorHandler | null = null;

/**
 * Get global error handler instance
 */
export function getErrorHandler(): CLIErrorHandler {
  if (!globalErrorHandler) {
    globalErrorHandler = new CLIErrorHandler();
  }
  return globalErrorHandler;
}

/**
 * Set global error handler
 */
export function setErrorHandler(handler: CLIErrorHandler): void {
  globalErrorHandler = handler;
}

/**
 * Convenience function: Handle error and exit
 */
export function handleError(error: unknown, context?: ErrorContext): never {
  return getErrorHandler().handleError(error, context);
}

/**
 * Convenience function: Format error message
 */
export function formatError(error: unknown, context?: ErrorContext): FormattedError {
  return getErrorHandler().formatError(error, context);
}
