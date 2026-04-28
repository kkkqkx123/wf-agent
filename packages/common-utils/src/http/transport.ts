/**
 * Transport Protocol Abstraction Layer
 * Defines a unified interface for HTTP and SSE transport protocols
 * Mainly used for REST API tools and streaming
 */

/**
 * Transport Response Interface
 */
export interface TransportResponse<T = unknown> {
  data: T;
  status?: number;
  headers?: Record<string, string>;
  requestId?: string;
}

/**
 * Transport Options Interface
 */
export interface TransportOptions {
  headers?: Record<string, string>;
  query?: Record<string, string | number | boolean>;
  timeout?: number;
  stream?: boolean;
  method?: "GET" | "POST" | "PUT" | "DELETE";
  body?: unknown;
}

/**
 * transport protocol interface
 */
export interface Transport {
  execute<T = unknown>(url: string, options?: TransportOptions): Promise<TransportResponse<T>>;
  executeStream?(url: string, options?: TransportOptions): AsyncIterable<unknown>;
}

/**
 * HTTP Transport Implementation
 */
export class HttpTransport implements Transport {
  constructor(
    private baseUrl?: string,
    private defaultHeaders?: Record<string, string>,
    private timeout?: number,
  ) {}

  async execute<T = unknown>(url: string, options?: TransportOptions): Promise<TransportResponse<T>> {
    // Constructing full URLs
    let fullUrl = url;
    if (this.baseUrl && !url.startsWith("http://") && !url.startsWith("https://")) {
      const cleanBaseUrl = this.baseUrl.replace(/\/$/, "");
      const cleanUrl = url.replace(/^\//, "");
      fullUrl = `${cleanBaseUrl}/${cleanUrl}`;
    }

    // Adding Query Parameters
    if (options?.query) {
      const queryString = new URLSearchParams(
        Object.entries(options.query)
          .filter(([_, value]) => value !== undefined && value !== null)
          .map(([key, value]) => [key, String(value)]),
      ).toString();

      fullUrl += `?${queryString}`;
    }

    // Merge request header
    const headers = {
      ...(this.defaultHeaders || {}),
      ...(options?.headers || {}),
    };

    // Creating an AbortController for timeouts
    const controller = new AbortController();
    const timeoutId =
      options?.timeout || this.timeout
        ? setTimeout(() => controller.abort(), options?.timeout || this.timeout)
        : null;

    try {
      const response = await fetch(fullUrl, {
        method: "GET", // Simplified for example, could extend to support other methods
        headers,
        signal: controller.signal,
      });

      if (timeoutId) clearTimeout(timeoutId);

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      // If a streaming response is requested, the return stream
      if (options?.stream) {
        return {
          data: response.body as T,
          status: response.status,
          headers: this.headersToObject(response.headers),
          requestId: response.headers.get("x-request-id") || undefined,
        };
      }

      // parse the response
      const contentType = response.headers.get("content-type");
      let data: T;
      if (contentType && contentType.includes("application/json")) {
        data = (await response.json()) as T;
      } else {
        data = (await response.text()) as T;
      }

      return {
        data,
        status: response.status,
        headers: this.headersToObject(response.headers),
        requestId: response.headers.get("x-request-id") || undefined,
      };
    } catch (error) {
      if (timeoutId) clearTimeout(timeoutId);
      throw error;
    }
  }

  /**
   * Converting Headers to Objects
   */
  private headersToObject(headers: Headers): Record<string, string> {
    const obj: Record<string, string> = {};
    headers.forEach((value, key) => {
      obj[key] = value;
    });
    return obj;
  }
}

/**
 * SSE Transport Implementation
 */
export class SseTransport implements Transport {
  constructor(
    private baseUrl?: string,
    private defaultHeaders?: Record<string, string>,
    private timeout?: number,
  ) {}

  async execute<T = unknown>(url: string, options?: TransportOptions): Promise<TransportResponse<T>> {
    // For SSE, we return an iterable stream
    const fullUrl = this.buildFullUrl(url, options?.query);
    const headers = {
      Accept: "text/event-stream, text/plain, */*",
      ...(this.defaultHeaders || {}),
      ...(options?.headers || {}),
    };

    const response = await fetch(fullUrl, {
      headers,
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    return {
      data: response.body as T,
      status: response.status,
      headers: this.headersToObject(response.headers),
      requestId: response.headers.get("x-request-id") || undefined,
    };
  }

  async *executeStream(url: string, options?: TransportOptions): AsyncIterable<unknown> {
    const fullUrl = this.buildFullUrl(url, options?.query);
    const headers = {
      Accept: "text/event-stream, text/plain, */*",
      ...(this.defaultHeaders || {}),
      ...(options?.headers || {}),
    };

    const response = await fetch(fullUrl, {
      method: options?.method || "GET",
      headers,
      body: options?.body ? JSON.stringify(options.body) : undefined,
    });

    if (!response.body) {
      throw new Error("Response body is null");
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          break;
        }

        // Adding new data to the buffer
        buffer += decoder.decode(value, { stream: true });

        // Split buffer contents by line
        const lines = buffer.split(/\r?\n/);
        buffer = lines.pop() || ""; // Retain incomplete last line

        // Processing each line
        for (const line of lines) {
          if (line.startsWith("data: ")) {
            // This is the SSE data line
            const data = line.substring(6); // Remove the "data: " prefix

            if (data === "[DONE]" || data.trim() === "") {
              // Special markers indicate end-of-stream or blank lines
              continue;
            }

            try {
              // Trying to parse JSON data
              yield JSON.parse(data);
            } catch {
              // If it's not JSON, treat it as a normal string.
              yield data;
            }
          }
          // Other SSE fields such as event:, id:, retry: can be handled here
        }
      }

      // Processing of the remaining data in the buffer
      if (buffer.trim()) {
        try {
          yield JSON.parse(buffer.trim());
        } catch {
          yield buffer.trim();
        }
      }
    } finally {
      reader.releaseLock();
    }
  }

  /**
   * Constructing full URLs
   */
  private buildFullUrl(url: string, query?: Record<string, string | number | boolean>): string {
    let fullUrl = url;

    if (this.baseUrl && !url.startsWith("http://") && !url.startsWith("https://")) {
      const cleanBaseUrl = this.baseUrl.replace(/\/$/, "");
      const cleanUrl = url.replace(/^\//, "");
      fullUrl = `${cleanBaseUrl}/${cleanUrl}`;
    }

    if (query && Object.keys(query).length > 0) {
      const queryString = new URLSearchParams(
        Object.entries(query)
          .filter(([_, value]) => value !== undefined && value !== null)
          .map(([key, value]) => [key, String(value)]),
      ).toString();

      fullUrl += `?${queryString}`;
    }

    return fullUrl;
  }

  /**
   * Converting Headers to Objects
   */
  private headersToObject(headers: Headers): Record<string, string> {
    const obj: Record<string, string> = {};
    headers.forEach((value, key) => {
      obj[key] = value;
    });
    return obj;
  }
}
