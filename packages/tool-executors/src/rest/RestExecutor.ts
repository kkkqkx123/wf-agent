/**
 * REST Tool Executor
 * Executes REST API calls, supporting all HTTP methods, interceptors, caching, circuit breakers, and more.
 */

import type { Tool, HTTPMethod, HttpRequestOptions } from "@wf-agent/types";
import type { RestToolConfig } from "@wf-agent/types";
import {
  NetworkError,
  ToolError,
  ValidationError,
  RuntimeValidationError,
  TimeoutError,
  CircuitBreakerOpenError,
} from "@wf-agent/types";
import { BaseExecutor } from "../core/base/BaseExecutor.js";
import { HttpClient, InterceptorManager } from "@wf-agent/common-utils";
import type { RestExecutorConfig } from "./types.js";

/**
 * REST Tool Executor
 */
export class RestExecutor extends BaseExecutor {
  private httpClient: HttpClient;
  private interceptorManager: InterceptorManager;
  private config: RestExecutorConfig;

  constructor(config: RestExecutorConfig = {}) {
    super();
    this.config = config;

    // Create an HttpClient for common-utils
    this.httpClient = new HttpClient({
      baseURL: config.baseUrl,
      defaultHeaders: config.headers,
      timeout: config.timeout,
      enableCircuitBreaker: config.enableCircuitBreaker,
      circuitBreakerFailureThreshold: config.circuitBreaker?.failureThreshold,
    });

    // Create an interceptor manager
    this.interceptorManager = new InterceptorManager();

    // Add an interceptor
    if (config.requestInterceptors) {
      config.requestInterceptors.forEach(interceptor => {
        this.interceptorManager.addRequestInterceptor(interceptor);
      });
    }

    if (config.responseInterceptors) {
      config.responseInterceptors.forEach(interceptor => {
        this.interceptorManager.addResponseInterceptor(interceptor);
      });
    }

    if (config.errorInterceptors) {
      config.errorInterceptors.forEach(interceptor => {
        this.interceptorManager.addErrorInterceptor(interceptor);
      });
    }
  }

  /**
   * Specific implementation of executing a REST tool
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @param threadId: Thread ID (optional; not used by the REST tool)
   * @returns: Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    _threadId?: string,
  ): Promise<unknown> {
    // Get the REST configuration from the config.
    const toolConfig = tool.config as RestToolConfig;

    // Get the request parameters from the parameters.
    const url = (parameters["url"] || parameters["endpoint"]) as string;
    const method = ((parameters["method"] || "GET") as string).toUpperCase();
    const body = parameters["body"];
    const headers = parameters["headers"] as Record<string, string> | undefined;
    const queryParams = parameters["query"] || parameters["params"];

    if (!url) {
      throw new RuntimeValidationError("URL is required for REST tool", {
        operation: "execute",
        field: "url",
        value: url,
        context: { toolId: tool.id, toolName: tool.name, parameters },
      });
    }

    try {
      // Constructing the request configuration
      const requestConfig = {
        url,
        method: method as HTTPMethod,
        headers,
        body,
        query: queryParams,
      };

      // Application request interceptor
      const processedConfig = (await this.interceptorManager.applyRequestInterceptors(
        requestConfig,
      )) as HttpRequestOptions;

      // Execute the request.
      let response;
      switch (method) {
        case "GET":
          response = await this.httpClient.get(url, processedConfig);
          break;
        case "POST":
          response = await this.httpClient.post(url, body, processedConfig);
          break;
        case "PUT":
          response = await this.httpClient.put(url, body, processedConfig);
          break;
        case "DELETE":
          response = await this.httpClient.delete(url, processedConfig);
          break;
        case "PATCH":
          response = await this.httpClient.post(url, body, processedConfig);
          break;
        case "HEAD":
          response = await this.httpClient.get(url, processedConfig);
          break;
        case "OPTIONS":
          response = await this.httpClient.get(url, processedConfig);
          break;
        default:
          throw new RuntimeValidationError(`Unsupported HTTP method: ${method}`, {
            operation: "execute",
            field: "method",
            value: method,
            context: { toolId: tool.id },
          });
      }

      // Application response interceptor
      const processedResponse = await this.interceptorManager.applyResponseInterceptors({
        status: response?.status ?? 0,
        statusText: response?.statusText,
        headers: response?.headers,
        data: response?.data,
        requestId: response?.requestId,
      });

      return this.formatResponse(toolConfig, url, method, processedResponse);
    } catch (error) {
      // Application error interceptor
      let processedError = error instanceof Error ? error : new Error(String(error));
      processedError = await this.interceptorManager.applyErrorInterceptors(processedError);

      // Translate error type
      if (processedError instanceof NetworkError || processedError instanceof ValidationError) {
        throw processedError;
      }

      if (processedError instanceof TimeoutError) {
        throw new ToolError(
          `REST tool execution timeout: ${processedError.message}`,
          tool.id,
          "REST",
          { url, method },
        );
      }

      if (processedError instanceof CircuitBreakerOpenError) {
        const circuitError = processedError as CircuitBreakerOpenError;
        throw new ToolError(
          `REST tool circuit breaker is open: ${circuitError.message}`,
          tool.id,
          "REST",
          { url, method },
          circuitError,
        );
      }

      throw new ToolError(
        `REST tool execution failed: ${processedError.message}`,
        tool.id,
        "REST",
        { url, method },
        processedError,
      );
    }
  }

  /**
   * Formatted response
   */
  private formatResponse(
    toolConfig: RestToolConfig,
    url: string,
    method: string,
    response: {
      status?: number;
      statusText?: string;
      headers?: Record<string, string>;
      data?: unknown;
      requestId?: string;
    },
  ): {
    url: string;
    method: string;
    status?: number;
    statusText?: string;
    headers?: Record<string, string>;
    data?: unknown;
    requestId?: string;
  } {
    return {
      url: this.buildFullUrl(toolConfig?.baseUrl || "", url),
      method,
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
      data: response.data,
      requestId: response.requestId,
    };
  }

  /**
   * Construct a complete URL
   */
  private buildFullUrl(
    baseUrl: string,
    url: string,
    queryParams?: Record<string, unknown>,
  ): string {
    // Merge the base URL with the endpoint.
    let fullUrl = url;

    if (baseUrl && !url.startsWith("http://") && !url.startsWith("https://")) {
      // Remove the slash at the end of baseUrl and the slash at the beginning of the url.
      const cleanBaseUrl = baseUrl.replace(/\/$/, "");
      const cleanUrl = url.replace(/^\//, "");
      fullUrl = `${cleanBaseUrl}/${cleanUrl}`;
    }

    // Add query parameters
    if (queryParams && Object.keys(queryParams).length > 0) {
      const queryString = new URLSearchParams(
        Object.entries(queryParams)
          .filter(([_, value]) => value !== undefined && value !== null)
          .map(([key, value]) => [key, String(value)]),
      ).toString();

      fullUrl += `?${queryString}`;
    }

    return fullUrl;
  }

  /**
   * Add a request interceptor
   */
  addRequestInterceptor(interceptor: {
    intercept: (config: unknown) => unknown | Promise<unknown>;
  }): void {
    this.interceptorManager.addRequestInterceptor(
      interceptor as import("@wf-agent/common-utils").RequestInterceptor,
    );
  }

  /**
   * Add a response interceptor
   */
  addResponseInterceptor(interceptor: {
    intercept: (response: unknown) => unknown | Promise<unknown>;
  }): void {
    this.interceptorManager.addResponseInterceptor(
      interceptor as import("@wf-agent/common-utils").ResponseInterceptor,
    );
  }

  /**
   * Add an error interceptor
   */
  addErrorInterceptor(interceptor: { intercept: (error: Error) => Error | Promise<Error> }): void {
    this.interceptorManager.addErrorInterceptor(interceptor);
  }

  /**
   * Obtain the executor type.
   */
  getExecutorType(): string {
    return "REST";
  }
}
