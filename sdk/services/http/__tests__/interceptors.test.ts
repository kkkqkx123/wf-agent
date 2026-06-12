import { describe, it, expect, beforeEach } from "vitest";
import {
  InterceptorManager,
  createAuthInterceptor,
  createLoggingInterceptor,
  createRetryInterceptor,
} from "../interceptors.js";
import type { RequestConfig, ResponseData } from "../interceptors.js";

describe("InterceptorManager", () => {
  let manager: InterceptorManager;

  beforeEach(() => {
    manager = new InterceptorManager();
  });

  describe("request interceptors", () => {
    it("should apply request interceptors in order", async () => {
      manager.addRequestInterceptor({
        intercept: config => ({ ...config, headers: { ...config.headers, a: "1" } }),
      });
      manager.addRequestInterceptor({
        intercept: config => ({ ...config, headers: { ...config.headers, b: "2" } }),
      });

      const result = await manager.applyRequestInterceptors({
        url: "/test",
        method: "GET",
        headers: {},
      });
      expect(result.headers).toEqual({ a: "1", b: "2" });
    });

    it("should support async interceptors", async () => {
      manager.addRequestInterceptor({
        intercept: async config => ({
          ...config,
          headers: { ...config.headers, async: "true" },
        }),
      });

      const result = await manager.applyRequestInterceptors({
        url: "/test",
        method: "GET",
      });
      expect(result.headers?.["async"]).toBe("true");
    });

    it("should return config unchanged when no interceptors", async () => {
      const config: RequestConfig = { url: "/test", method: "GET" };
      const result = await manager.applyRequestInterceptors(config);
      expect(result).toEqual(config);
    });
  });

  describe("response interceptors", () => {
    it("should apply response interceptors in order", async () => {
      manager.addResponseInterceptor({
        intercept: resp => ({ ...resp, status: resp.status + 1 }),
      });
      manager.addResponseInterceptor({
        intercept: resp => ({ ...resp, data: { transformed: true } }),
      });

      const result = await manager.applyResponseInterceptors({
        status: 200,
        data: { original: true },
      });
      expect(result.status).toBe(201);
      expect(result.data).toEqual({ transformed: true });
    });

    it("should return response unchanged when no interceptors", async () => {
      const response: ResponseData = { status: 200, data: "hello" };
      const result = await manager.applyResponseInterceptors(response);
      expect(result).toEqual(response);
    });
  });

  describe("error interceptors", () => {
    it("should apply error interceptors", async () => {
      manager.addErrorInterceptor({
        intercept: error => {
          error.message = `Transformed: ${error.message}`;
          return error;
        },
      });

      const result = await manager.applyErrorInterceptors(new Error("original"));
      expect(result.message).toBe("Transformed: original");
    });
  });

  describe("clear", () => {
    it("should remove all interceptors", async () => {
      manager.addRequestInterceptor({
        intercept: config => ({ ...config, url: "/modified" }),
      });
      manager.clear();

      const result = await manager.applyRequestInterceptors({ url: "/test", method: "GET" });
      expect(result.url).toBe("/test");
    });
  });
});

describe("createAuthInterceptor", () => {
  it("should add Bearer token by default", () => {
    const interceptor = createAuthInterceptor("my-token");
    const result = interceptor.intercept({ url: "/api", method: "GET" });
    expect((result as RequestConfig).headers).toEqual({ Authorization: "Bearer my-token" });
  });

  it("should support custom scheme", () => {
    const interceptor = createAuthInterceptor("key-123", "ApiKey");
    const result = interceptor.intercept({ url: "/api", method: "GET" });
    expect((result as RequestConfig).headers).toEqual({ Authorization: "ApiKey key-123" });
  });

  it("should preserve existing headers", () => {
    const interceptor = createAuthInterceptor("tok");
    const result = interceptor.intercept({
      url: "/api",
      method: "POST",
      headers: { "Content-Type": "application/json" },
    });
    expect((result as RequestConfig).headers).toEqual({
      "Content-Type": "application/json",
      Authorization: "Bearer tok",
    });
  });
});

describe("createLoggingInterceptor", () => {
  it("should log request, response, and error", () => {
    const logs: string[] = [];
    const logger = (msg: string) => logs.push(msg);

    const { request, response, error } = createLoggingInterceptor(logger);

    // Request
    request.intercept({ url: "/api", method: "GET" });
    expect(logs[0]).toContain("[Request]");

    // Response
    response.intercept({ status: 200 });
    expect(logs[1]).toContain("[Response]");

    // Error
    error.intercept(new Error("fail"));
    expect(logs[2]).toContain("[Error]");
  });
});

describe("createRetryInterceptor", () => {
  it("should mark errors as retryable", async () => {
    const shouldRetry = (_err: Error, _count: number) => true;
    const getDelay = (_count: number) => 100;
    const interceptor = createRetryInterceptor(shouldRetry, getDelay);

    const result = await interceptor.intercept(new Error("temp failure"));
    expect((result as Error & { retryable?: boolean }).retryable).toBe(true);
  });
});
