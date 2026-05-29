import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { HttpClient } from "../http-client.js";
import type { HttpRequestOptions } from "@wf-agent/types";

describe("HttpClient", () => {
  let client: HttpClient;
  let originalFetch: typeof globalThis.fetch;

  const mockFetch = (status: number, body: unknown, headers?: Record<string, string>) => {
    const responseHeaders = new Headers(headers || { "content-type": "application/json" });
    const responseBody = typeof body === "string" ? body : JSON.stringify(body);
    const mockResponse = new Response(responseBody, {
      status,
      statusText: status === 200 ? "OK" : "Error",
      headers: responseHeaders,
    });
    globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);
  };

  const mockFetchError = (error: Error) => {
    globalThis.fetch = vi.fn().mockRejectedValue(error);
  };

  beforeEach(() => {
    originalFetch = globalThis.fetch;
    client = new HttpClient({ baseURL: "https://api.test.com", timeout: 5000 });
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  describe("HTTP methods", () => {
    it("should make GET request", async () => {
      mockFetch(200, { data: "ok" });
      const result = await client.get("/test");
      expect(result.data).toEqual({ data: "ok" });
      expect(globalThis.fetch).toHaveBeenCalledWith(
        "https://api.test.com/test",
        expect.objectContaining({ method: "GET" }),
      );
    });

    it("should make POST request with body", async () => {
      mockFetch(201, { id: 1 });
      const result = await client.post("/create", { name: "test" });
      expect(result.status).toBe(201);
      expect(globalThis.fetch).toHaveBeenCalledWith(
        "https://api.test.com/create",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ name: "test" }),
        }),
      );
    });

    it("should make PUT request", async () => {
      mockFetch(200, { updated: true });
      const result = await client.put("/update/1", { name: "new" });
      expect(result.data).toEqual({ updated: true });
    });

    it("should make DELETE request", async () => {
      mockFetch(200, { deleted: true });
      const result = await client.delete("/delete/1");
      expect(result.status).toBe(200);
    });

    it("should make PATCH request", async () => {
      mockFetch(200, { patched: true });
      const result = await client.patch("/patch/1", { field: "val" });
      expect(result.data).toEqual({ patched: true });
    });

    it("should make HEAD request", async () => {
      const headHeaders = new Headers({ "content-type": "text/plain" });
      globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 200, headers: headHeaders }));
      await client.head("/check");
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({ method: "HEAD" }),
      );
    });

    it("should make OPTIONS request", async () => {
      const optHeaders = new Headers({ "content-type": "text/plain" });
      globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 200, headers: optHeaders }));
      await client.options("/options");
      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({ method: "OPTIONS" }),
      );
    });
  });

  describe("request configuration", () => {
    it("should merge default headers", async () => {
      client = new HttpClient({
        baseURL: "https://api.test.com",
        defaultHeaders: { "X-App": "test", "Content-Type": "application/json" },
      });
      mockFetch(200, {});

      await client.get("/test");
      const callHeaders = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]![1].headers;
      expect(callHeaders["X-App"]).toBe("test");
      expect(callHeaders["Content-Type"]).toBe("application/json");
    });

    it("should override default headers with per-request headers", async () => {
      client = new HttpClient({
        baseURL: "https://api.test.com",
        defaultHeaders: { Authorization: "Bearer default" },
      });
      mockFetch(200, {});

      const options: HttpRequestOptions = { headers: { Authorization: "Bearer custom" } };
      await client.get("/test", options);
      const callHeaders = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]![1].headers;
      expect(callHeaders["Authorization"]).toBe("Bearer custom");
    });

    it("should add query parameters", async () => {
      mockFetch(200, {});
      const options: HttpRequestOptions = { query: { page: "1", limit: 10 } };
      await client.get("/items", options);
      const calledUrl = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]![0];
      expect(calledUrl).toBe("https://api.test.com/items?page=1&limit=10");
    });
  });

  describe("error handling", () => {
    it("should throw BadRequestError on 400", async () => {
      mockFetch(400, "Bad Request");
      const { BadRequestError } = await import("../errors.js");
      await expect(client.get("/test")).rejects.toThrow(BadRequestError);
    });

    it("should throw UnauthorizedError on 401", async () => {
      mockFetch(401, "Unauthorized");
      const { UnauthorizedError } = await import("../errors.js");
      await expect(client.get("/test")).rejects.toThrow(UnauthorizedError);
    });

    it("should throw NotFoundHttpError on 404", async () => {
      mockFetch(404, "Not Found");
      const { NotFoundHttpError } = await import("../errors.js");
      await expect(client.get("/test")).rejects.toThrow(NotFoundHttpError);
    });

    it("should throw InternalServerError on 500", async () => {
      const noRetryClient = new HttpClient({ baseURL: "https://api.test.com", timeout: 5000, maxRetries: 0 });
      mockFetch(500, "Server Error");
      const { InternalServerError } = await import("../errors.js");
      await expect(noRetryClient.get("/test")).rejects.toThrow(InternalServerError);
    });

    it("should throw ServiceUnavailableError on 503", async () => {
      const noRetryClient = new HttpClient({ baseURL: "https://api.test.com", timeout: 5000, maxRetries: 0 });
      mockFetch(503, "Unavailable");
      const { ServiceUnavailableError } = await import("../errors.js");
      await expect(noRetryClient.get("/test")).rejects.toThrow(ServiceUnavailableError);
    });
  });

  describe("stream option", () => {
    it("should return response body as data when stream=true", async () => {
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(new TextEncoder().encode("chunk"));
          controller.close();
        },
      });
      const mockResponse = new Response(stream, {
        status: 200,
        headers: { "content-type": "text/plain" },
      });
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

      const result = await client.get("/stream", { stream: true });
      expect(result.data).toBeInstanceOf(ReadableStream);
    });
  });

  describe("interceptors", () => {
    it("should apply request interceptor", async () => {
      mockFetch(200, {});
      client.interceptors.addRequestInterceptor({
        intercept: (config) => ({
          ...config,
          headers: { ...config.headers, "X-Interceptor": "applied" },
        }),
      });

      await client.get("/test");
      const callHeaders = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]![1].headers;
      expect(callHeaders["X-Interceptor"]).toBe("applied");
    });

    it("should apply response interceptor", async () => {
      mockFetch(200, { original: true });
      client.interceptors.addResponseInterceptor({
        intercept: (resp) => ({
          ...resp,
          data: { ...(resp.data as object), interceptor: true },
        }),
      });

      const result = await client.get<{ original: boolean; interceptor: boolean }>("/test");
      expect(result.data.interceptor).toBe(true);
      expect(result.data.original).toBe(true);
    });

    it("should apply error interceptor", async () => {
      mockFetchError(new Error("network failure"));
      client.interceptors.addErrorInterceptor({
        intercept: (error) => {
          error.message = `Enhanced: ${error.message}`;
          return error;
        },
      });

      await expect(client.get("/test")).rejects.toThrow("Enhanced: network failure");
    });
  });

  describe("circuit breaker", () => {
    it("should throw CircuitBreakerOpenError when circuit is open", async () => {
      client = new HttpClient({
        enableCircuitBreaker: true,
        circuitBreakerFailureThreshold: 2,
      });

      // Cause failures to open the circuit
      for (let i = 0; i < 2; i++) {
        mockFetch(500, "error");
        await expect(client.get("/fail")).rejects.toThrow();
      }

      // Circuit is now open
      mockFetch(200, { ok: true });
      const { CircuitBreakerOpenError } = await import("@wf-agent/types");
      await expect(client.get("/fail")).rejects.toThrow(CircuitBreakerOpenError);
    });
  });

  describe("configuration defaults", () => {
    it("should use sensible defaults", async () => {
      const defaultClient = new HttpClient();
      mockFetch(200, {});

      await defaultClient.get("https://api.test.com/data");
      // Should work without baseURL (uses full URL)
      expect(globalThis.fetch).toHaveBeenCalledWith(
        "https://api.test.com/data",
        expect.any(Object),
      );
    });

    it("should respect custom timeout", async () => {
      // Make fetch respect the abort signal by hanging until timeout fires
      globalThis.fetch = vi.fn().mockImplementation((_url: string, options: RequestInit) => {
        return new Promise<Response>((_resolve, reject) => {
          const signal = options.signal as AbortSignal;
          if (signal.aborted) {
            reject(new DOMException("The operation was aborted", "AbortError"));
            return;
          }
          signal.addEventListener("abort", () => {
            reject(new DOMException("The operation was aborted", "AbortError"));
          }, { once: true });
          // Never resolve — the signal.abort() after timeout will reject
        });
      });

      client = new HttpClient({ timeout: 10, maxRetries: 0 });
      await expect(client.get("https://slow.example.com")).rejects.toThrow();
    }, 5000);
  });
});
