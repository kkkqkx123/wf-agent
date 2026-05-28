import { describe, it, expect } from "vitest";
import { HttpError, TimeoutError, NetworkError } from "@wf-agent/types";
import { executeWithRetry } from "../retry-handler.js";
import {
  InternalServerError,
  RateLimitError as HttpRateLimitError,
  ServiceUnavailableError,
} from "../errors.js";

describe("executeWithRetry", () => {
  it("should return successful result on first try", async () => {
    const fn = async () => "success";
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("success");
  });

  it("should retry on TimeoutError", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 3) throw new TimeoutError("timeout", 5000);
      return "recovered";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("recovered");
    expect(attempts).toBe(3);
  });

  it("should retry on NetworkError", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 2) throw new NetworkError("network failure", { code: "ECONNRESET" });
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("ok");
    expect(attempts).toBe(2);
  });

  it("should retry on 429 RateLimitError", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 2) throw new HttpRateLimitError("rate limited");
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("ok");
  });

  it("should retry on 500 InternalServerError", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 2) throw new InternalServerError("server error");
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("ok");
  });

  it("should retry on 503 ServiceUnavailableError", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 2) throw new ServiceUnavailableError("unavailable");
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("ok");
  });

  it("should retry on 5xx HttpError (generic)", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 2) throw new HttpError("502 bad gateway", 502);
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 });
    expect(result).toBe("ok");
  });

  it("should NOT retry on 400 BadRequest", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new HttpError("bad request", 400);
    };
    await expect(executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(1);
  });

  it("should NOT retry on 401 Unauthorized", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new HttpError("unauthorized", 401);
    };
    await expect(executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(1);
  });

  it("should NOT retry on 403 Forbidden", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new HttpError("forbidden", 403);
    };
    await expect(executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(1);
  });

  it("should NOT retry on 404 Not Found", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new HttpError("not found", 404);
    };
    await expect(executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(1);
  });

  it("should throw after max attempts", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new TimeoutError("timeout", 5000);
    };
    await expect(executeWithRetry(fn, { maxRetries: 2, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(3); // 1 initial + 2 retries
  });

  it("should use maxDelay to cap exponential backoff", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      if (attempts < 3) throw new TimeoutError("timeout", 5000);
      return "ok";
    };
    const result = await executeWithRetry(fn, { maxRetries: 3, baseDelay: 100, maxDelay: 200 });
    expect(result).toBe("ok");
  });

  it("should not retry on non-retryable error (e.g., SyntaxError)", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new SyntaxError("parse error");
    };
    await expect(executeWithRetry(fn, { maxRetries: 3, baseDelay: 10 })).rejects.toThrow(SyntaxError);
    expect(attempts).toBe(1);
  });

  it("should handle 0 maxRetries (no retry)", async () => {
    let attempts = 0;
    const fn = async () => {
      attempts++;
      throw new TimeoutError("timeout", 5000);
    };
    await expect(executeWithRetry(fn, { maxRetries: 0, baseDelay: 10 })).rejects.toThrow();
    expect(attempts).toBe(1);
  });
});
