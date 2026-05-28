/**
 * HTTP SSE Transport
 * Handles HTTP Server-Sent Events streaming transport
 * Used primarily by LLM clients for streaming responses
 */

/**
 * Transport Response type
 */
interface TransportResponse<T = unknown> {
  data: T;
  status: number;
  headers?: Record<string, string>;
  requestId?: string;
}

/**
 * SSE Transport Implementation
 * Provides SSE streaming over HTTP
 */
export class SseTransport {
  constructor(
    private baseUrl?: string,
    private defaultHeaders?: Record<string, string>,
    private timeout: number = 30000,
  ) {}

  /**
   * Execute an SSE streaming request
   */
  async *executeStream<T = unknown>(
    url: string,
    options?: {
      query?: Record<string, string | number | boolean>;
      method?: string;
      body?: unknown;
      headers?: Record<string, string>;
      timeout?: number;
    },
  ): AsyncIterable<T> {
    const fullUrl = this.buildFullUrl(url, options?.query);
    const headers: Record<string, string> = {
      Accept: "text/event-stream, text/plain, */*",
      ...(this.defaultHeaders || {}),
      ...(options?.headers || {}),
    };

    const controller = new AbortController();
    const timeoutMs = options?.timeout || this.timeout;
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const fetchOptions: RequestInit & { headers: Record<string, string> } = {
        method: options?.method || "GET",
        headers,
        signal: controller.signal,
      };

      if (options?.body !== undefined) {
        if (typeof options.body === "string") {
          fetchOptions.body = options.body;
        } else {
          fetchOptions.body = JSON.stringify(options.body);
          if (!headers["Content-Type"]) {
            headers["Content-Type"] = "application/json";
          }
        }
      }

      const response = await fetch(fullUrl, fetchOptions);

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      if (!response.body) {
        throw new Error("Response body is null");
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      try {
        while (true) {
          // Race reader.read() against abort signal
          const readPromise = reader.read();
          const abortPromise = new Promise<never>((_, reject) => {
            if (controller.signal.aborted) {
              reject(new DOMException("The operation was aborted", "AbortError"));
            } else {
              controller.signal.addEventListener("abort", () => {
                reject(new DOMException("The operation was aborted", "AbortError"));
              }, { once: true });
            }
          });

          const { done, value } = await Promise.race([readPromise, abortPromise]);
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() || ""; // Keep incomplete line in buffer

          for (const line of lines) {
            const parsed = this.parseSSELine(line);
            if (parsed !== null) {
              yield parsed as T;
            }
          }
        }
      } finally {
        reader.releaseLock();
      }
    } finally {
      clearTimeout(timeoutId);
    }
  }

  /**
   * Execute an SSE request and return all data as array
   */
  async execute<T = unknown>(
    url: string,
    options?: {
      query?: Record<string, string | number | boolean>;
      method?: string;
      body?: unknown;
      headers?: Record<string, string>;
      timeout?: number;
    },
  ): Promise<TransportResponse<T>> {
    const fullUrl = this.buildFullUrl(url, options?.query);
    const headers: Record<string, string> = {
      Accept: "text/event-stream, text/plain, */*",
      ...(this.defaultHeaders || {}),
      ...(options?.headers || {}),
    };

    const controller = new AbortController();
    const timeoutMs = options?.timeout || this.timeout;
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const fetchOptions: RequestInit & { headers: Record<string, string> } = {
        method: options?.method || "GET",
        headers,
        signal: controller.signal,
      };

      if (options?.body !== undefined) {
        if (typeof options.body === "string") {
          fetchOptions.body = options.body;
        } else {
          fetchOptions.body = JSON.stringify(options.body);
        }
      }

      const response = await fetch(fullUrl, fetchOptions);

      if (timeoutId) clearTimeout(timeoutId);

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      return {
        data: response.body as T,
        status: response.status,
        headers: this.headersToObject(response.headers),
        requestId: response.headers.get("x-request-id") || undefined,
      };
    } catch (error) {
      clearTimeout(timeoutId);
      throw error;
    }
  }

  /**
   * Parse a single SSE line
   */
  private parseSSELine(line: string): unknown | null {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith(":")) {
      return null; // Comment or empty line
    }

    // Handle "data: ..." format
    if (trimmed.startsWith("data: ")) {
      const dataStr = trimmed.slice(6);
      try {
        return JSON.parse(dataStr);
      } catch {
        return dataStr;
      }
    }

    // Handle "event: ..." or other SSE fields
    // For now, return the raw line
    return trimmed;
  }

  /**
   * Build full URL with query parameters
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
   * Convert Headers to plain object
   */
  private headersToObject(headers: Headers): Record<string, string> {
    const result: Record<string, string> = {};
    headers.forEach((value, key) => {
      result[key] = value;
    });
    return result;
  }
}
