import { describe, it, expect } from "vitest";
import { createTransport, isTransportTypeSupported } from "../index.js";
import { StdioTransport } from "../stdio.js";
import { SseTransport } from "../sse.js";
import { StreamableHttpTransport } from "../streamable-http.js";

describe("createTransport", () => {
  it("should create StdioTransport for stdio config", () => {
    const transport = createTransport({ type: "stdio", command: "echo" });
    expect(transport).toBeInstanceOf(StdioTransport);
    expect(transport.type).toBe("stdio");
  });

  it("should create SseTransport for sse config", () => {
    const transport = createTransport({ type: "sse", url: "https://example.com/sse" });
    expect(transport).toBeInstanceOf(SseTransport);
    expect(transport.type).toBe("sse");
  });

  it("should create StreamableHttpTransport for streamable-http config", () => {
    const transport = createTransport({ type: "streamable-http", url: "https://example.com/mcp" });
    expect(transport).toBeInstanceOf(StreamableHttpTransport);
    expect(transport.type).toBe("streamable-http");
  });

  it("should throw for unknown transport type", () => {
    expect(() =>
      createTransport({ type: "unknown" as "stdio", command: "echo" }),
    ).toThrow("Unknown transport type");
  });
});

describe("isTransportTypeSupported", () => {
  it("should return true for stdio", () => {
    expect(isTransportTypeSupported("stdio")).toBe(true);
  });

  it("should return true for sse", () => {
    expect(isTransportTypeSupported("sse")).toBe(true);
  });

  it("should return true for streamable-http", () => {
    expect(isTransportTypeSupported("streamable-http")).toBe(true);
  });

  it("should return false for unknown types", () => {
    expect(isTransportTypeSupported("websocket")).toBe(false);
    expect(isTransportTypeSupported("")).toBe(false);
    expect(isTransportTypeSupported("unknown")).toBe(false);
  });
});
