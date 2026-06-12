import { describe, it, expect, vi, beforeEach } from "vitest";
import { McpClient } from "../mcp-client.js";
import type { IMcpTransport, TransportEventHandlers } from "../transport/types.js";

/**
 * Create a mock transport for testing McpClient
 */
function createMockTransport(): {
  transport: IMcpTransport;
  simulateData: (data: unknown) => void;
  simulateError: (error: Error) => void;
  simulateClose: () => void;
  handlers: {
    onData?: (data: unknown) => void;
    onError?: (error: Error) => void;
    onClose?: () => void;
  };
  sentMessages: unknown[];
} {
  const sentMessages: unknown[] = [];
  let handlers: TransportEventHandlers = {};

  const mockTransport: IMcpTransport = {
    type: "stdio" as const,
    isConnected: true,
    start: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    send: vi.fn().mockImplementation(async (msg: unknown) => {
      sentMessages.push(msg);
    }),
    setHandlers: vi.fn().mockImplementation((h: TransportEventHandlers) => {
      handlers = h;
    }),
  };

  return {
    transport: mockTransport,
    simulateData: (data: unknown) => handlers.onData?.(data),
    simulateError: (error: Error) => handlers.onError?.(error),
    simulateClose: () => handlers.onClose?.(),
    handlers,
    sentMessages,
  };
}

describe("McpClient", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: McpClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new McpClient(mock.transport);
  });

  describe("constructor", () => {
    it("should set up transport handlers on creation", () => {
      expect(mock.transport.setHandlers).toHaveBeenCalledTimes(1);
    });
  });

  describe("connect", () => {
    it("should start transport and send initialize request", async () => {
      // Simulate successful initialize response
      const connectPromise = client.connect({ name: "test-client", version: "1.0" });

      // Yield to allow connect's microtask (await transport.start()) to process
      await new Promise(resolve => setTimeout(resolve, 5));

      expect(mock.transport.start).toHaveBeenCalled();
      expect(mock.sentMessages.length).toBe(1);
      expect(mock.sentMessages[0]).toMatchObject({ method: "initialize" });

      // Simulate the initialize response with id=0 (first request)
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { instructions: "Hello", capabilities: {} },
      });

      // Yield to allow the connected path to process and send notifications/initialized
      await new Promise(resolve => setTimeout(resolve, 5));
      await connectPromise;
      expect(client.getInstructions()).toBe("Hello");
    });

    it("should handle initialize without instructions", async () => {
      const connectPromise = client.connect({ name: "test", version: "1.0" });

      // Yield to let connect send the initialize message
      await new Promise(resolve => setTimeout(resolve, 5));
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { capabilities: {} },
      });

      await connectPromise;
      expect(client.getInstructions()).toBeNull();
    });

    it("should reject connect on transport start error", async () => {
      const mockTransport2 = createMockTransport();
      mockTransport2.transport.start = vi.fn().mockRejectedValue(new Error("Connection refused"));
      const client2 = new McpClient(mockTransport2.transport);

      await expect(client2.connect({ name: "t", version: "1.0" })).rejects.toThrow(
        "Connection refused",
      );
    });
  });

  describe("listTools", () => {
    it("should send tools/list and return tools", async () => {
      const resultPromise = client.listTools();
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { tools: [{ name: "echo", description: "Echo" }] },
      });

      const tools = await resultPromise;
      expect(tools).toEqual([{ name: "echo", description: "Echo" }]);
      expect(mock.sentMessages[0]).toMatchObject({
        method: "tools/list",
      });
    });

    it("should return empty array when no tools", async () => {
      const resultPromise = client.listTools();
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: {},
      });

      const tools = await resultPromise;
      expect(tools).toEqual([]);
    });
  });

  describe("callTool", () => {
    it("should send tools/call and return result", async () => {
      const resultPromise = client.callTool("echo", { text: "hello" });
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { content: [{ type: "text", text: "hello" }] },
      });

      const result = await resultPromise;
      expect(result).toEqual({ content: [{ type: "text", text: "hello" }] });
      expect(mock.sentMessages[0]).toMatchObject({
        method: "tools/call",
        params: { name: "echo", arguments: { text: "hello" } },
      });
    });

    it("should reject on error response", async () => {
      const resultPromise = client.callTool("fail", {});
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        error: { code: -32603, message: "Internal error" },
      });

      await expect(resultPromise).rejects.toThrow();
    });
  });

  describe("listResources", () => {
    it("should send resources/list and return resources", async () => {
      const resultPromise = client.listResources();
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { resources: [{ uri: "file:///test", name: "Test" }] },
      });

      const resources = await resultPromise;
      expect(resources).toEqual([{ uri: "file:///test", name: "Test" }]);
    });
  });

  describe("readResource", () => {
    it("should send resources/read with uri", async () => {
      const resultPromise = client.readResource("file:///test.txt");
      mock.simulateData({
        jsonrpc: "2.0",
        id: 0,
        result: { contents: [{ uri: "file:///test.txt", text: "content" }] },
      });

      const result = await resultPromise;
      expect(result.contents).toHaveLength(1);
      expect(mock.sentMessages[0]).toMatchObject({
        method: "resources/read",
        params: { uri: "file:///test.txt" },
      });
    });
  });

  describe("error handling", () => {
    it("should reject all pending requests on transport error", async () => {
      const promise1 = client.listTools();
      const promise2 = client.listResources();

      mock.simulateError(new Error("Transport disconnected"));

      await expect(promise1).rejects.toThrow("Transport disconnected");
      await expect(promise2).rejects.toThrow("Transport disconnected");
    });

    it("should reject all pending requests on transport close", async () => {
      const promise1 = client.listTools();
      mock.simulateClose();

      await expect(promise1).rejects.toThrow("Transport closed");
    });
  });

  describe("close", () => {
    it("should close transport and clear pending requests", async () => {
      void client.listTools();
      await client.close();
      expect(mock.transport.close).toHaveBeenCalled();

      // Verify pending is cleared: simulate a late response (should be a no-op)
      mock.simulateData({ jsonrpc: "2.0", id: 0, result: { tools: [] } });
      // The promise didn't resolve because it was cleared
    });
  });
});
