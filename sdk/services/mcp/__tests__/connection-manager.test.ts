import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { McpConnectionManager } from "../connection-manager.js";

// Use vi.hoisted to create the mock class before vi.mock is evaluated
const MockMcpClient = vi.hoisted(() => {
  return class MockMcpClient {
    connect = vi.fn().mockResolvedValue(undefined);
    close = vi.fn().mockResolvedValue(undefined);
    listTools = vi
      .fn()
      .mockResolvedValue([{ name: "echo", description: "Echo test tool", inputSchema: {} }]);
    listResources = vi.fn().mockResolvedValue([]);
    listResourceTemplates = vi.fn().mockResolvedValue([]);
    callTool = vi.fn().mockResolvedValue({ content: [{ type: "text", text: "done" }] });
    readResource = vi.fn().mockResolvedValue({ contents: [{ uri: "test:///", text: "content" }] });
    getInstructions = vi.fn().mockReturnValue(null);
    constructor(_transport: unknown) {
      // Each new instance gets fresh mocks
      this.connect = vi.fn().mockResolvedValue(undefined);
      this.close = vi.fn().mockResolvedValue(undefined);
      this.listTools = vi
        .fn()
        .mockResolvedValue([{ name: "echo", description: "Echo test tool", inputSchema: {} }]);
      this.listResources = vi.fn().mockResolvedValue([]);
      this.listResourceTemplates = vi.fn().mockResolvedValue([]);
      this.callTool = vi.fn().mockResolvedValue({ content: [{ type: "text", text: "done" }] });
      this.readResource = vi
        .fn()
        .mockResolvedValue({ contents: [{ uri: "test:///", text: "content" }] });
      this.getInstructions = vi.fn().mockReturnValue(null);
    }
  };
});

function createMockTransport() {
  return {
    type: "stdio" as const,
    isConnected: false,
    start: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    send: vi.fn().mockResolvedValue(undefined),
    setHandlers: vi.fn(),
  };
}

// Mock the modules before imports are resolved
vi.mock("../mcp-client.js", () => ({
  McpClient: MockMcpClient,
}));

vi.mock("../transport/index.js", () => ({
  createTransport: vi.fn().mockImplementation(() => createMockTransport()),
}));

describe("McpConnectionManager", () => {
  let manager: McpConnectionManager;

  beforeEach(() => {
    vi.clearAllMocks();
    manager = new McpConnectionManager(
      { name: "test-client", version: "1.0" },
      { connectionTimeout: 5000 },
    );
  });

  afterEach(async () => {
    await manager.disconnectAll();
  });

  describe("constructor", () => {
    it("should create with defaults", () => {
      const m = new McpConnectionManager({ name: "app", version: "1.0" });
      expect(m.isMcpEnabled()).toBe(true);
    });

    it("should respect options", () => {
      const m = new McpConnectionManager({ name: "app", version: "1.0" }, { mcpEnabled: false });
      expect(m.isMcpEnabled()).toBe(false);
    });
  });

  describe("isMcpEnabled / setMcpEnabled", () => {
    it("should toggle MCP enabled state", () => {
      expect(manager.isMcpEnabled()).toBe(true);
      manager.setMcpEnabled(false);
      expect(manager.isMcpEnabled()).toBe(false);
      manager.setMcpEnabled(true);
      expect(manager.isMcpEnabled()).toBe(true);
    });
  });

  describe("connectServer", () => {
    it("should connect a stdio server", async () => {
      await manager.connectServer("echo", {
        type: "stdio",
        command: "node",
        args: ["-e", "console.log('ready')"],
        lifecycle: "eager",
      });

      const state = manager.getServerState("echo");
      expect(state).toBeDefined();
      expect(state!.status).toBe("connected");
      expect(state!.name).toBe("echo");
    });

    it("should create disabled entry when MCP is disabled", async () => {
      manager.setMcpEnabled(false);
      await manager.connectServer("test", { type: "stdio", command: "echo" });

      const state = manager.getServerState("test");
      expect(state).toBeDefined();
      expect(state!.disabled).toBe(true);
      expect(state!.status).toBe("disconnected");
    });

    it("should create disabled entry when config has disabled flag", async () => {
      await manager.connectServer("test", { type: "stdio", command: "echo", disabled: true });

      const state = manager.getServerState("test");
      expect(state).toBeDefined();
      expect(state!.disabled).toBe(true);
    });

    it("should emit events during connection", async () => {
      const events: string[] = [];
      manager.addEventHandler((event) => events.push(event.type));

      await manager.connectServer("echo", { type: "stdio", command: "echo", lifecycle: "eager" });
      expect(events).toContain("server:connecting");
      expect(events).toContain("server:connected");
    });

    it("should not auto-connect in lazy mode", async () => {
      await manager.connectServer(
        "lazy-server",
        { type: "stdio", command: "echo", lifecycle: "lazy" },
        "global"
      );

      const state = manager.getServerState("lazy-server");
      expect(state).toBeDefined();
      expect(state!.status).toBe("disconnected");
    });

    it("should auto-connect in eager mode", async () => {
      await manager.connectServer(
        "eager-server",
        { type: "stdio", command: "echo", lifecycle: "eager" },
        "global"
      );

      const state = manager.getServerState("eager-server");
      expect(state).toBeDefined();
      expect(state!.status).toBe("connected");
    });

    it("should auto-connect in keep-alive mode", async () => {
      await manager.connectServer(
        "keepalive-server",
        { type: "stdio", command: "echo", lifecycle: "keep-alive" },
        "global"
      );

      const state = manager.getServerState("keepalive-server");
      expect(state).toBeDefined();
      expect(state!.status).toBe("connected");
    });

    it("should respect manager-level defaultLifecycle", async () => {
      const mgr = new McpConnectionManager(
        { name: "app", version: "1.0" },
        { defaultLifecycle: "eager", connectionTimeout: 5000 }
      );

      await mgr.connectServer(
        "default-eager",
        { type: "stdio", command: "echo" },
        "global"
      );

      const state = mgr.getServerState("default-eager");
      expect(state!.status).toBe("connected");
      await mgr.disconnectAll();
    });
  });

  describe("lifecycle — lazy auto-connect on callTool", () => {
    it("should auto-connect lazy server on first callTool", async () => {
      await manager.connectServer(
        "lazy",
        { type: "stdio", command: "echo", lifecycle: "lazy" },
        "global"
      );

      const before = manager.getServerState("lazy");
      expect(before!.status).toBe("disconnected");

      await manager.callTool("lazy", "echo", {});

      const after = manager.getServerState("lazy");
      expect(after!.status).toBe("connected");
    });

    it("should throw for unregistered server on callTool", async () => {
      await expect(manager.callTool("nonexistent", "tool")).rejects.toThrow(
        "No server registered"
      );
    });

    it("should throw for unregistered server on readResource", async () => {
      await expect(manager.readResource("nonexistent", "uri")).rejects.toThrow(
        "No server registered"
      );
    });
  });

  describe("disconnectServer", () => {
    it("should disconnect a connected server", async () => {
      await manager.connectServer("echo", { type: "stdio", command: "echo" });
      expect(manager.getServerState("echo")).toBeDefined();

      await manager.disconnectServer("echo");
      expect(manager.getServerState("echo")).toBeUndefined();
    });

    it("should be no-op for unknown server", async () => {
      await expect(manager.disconnectServer("nonexistent")).resolves.toBeUndefined();
    });
  });

  describe("disconnectAll", () => {
    it("should disconnect all servers", async () => {
      await manager.connectServer("s1", { type: "stdio", command: "echo" });
      await manager.connectServer("s2", { type: "stdio", command: "echo" });

      await manager.disconnectAll();
      expect(manager.getAllServerStates()).toHaveLength(0);
    });
  });

  describe("getAllServerStates", () => {
    it("should return all server states", async () => {
      await manager.connectServer("s1", { type: "stdio", command: "echo" });
      await manager.connectServer("s2", { type: "stdio", command: "echo" });

      const states = manager.getAllServerStates();
      expect(states).toHaveLength(2);
    });
  });

  describe("getConnectedServers", () => {
    it("should return only connected servers", async () => {
      await manager.connectServer("s1", { type: "stdio", command: "echo", lifecycle: "eager" });

      await manager.connectServer("disabled", { type: "stdio", command: "echo", disabled: true });

      const connected = manager.getConnectedServers();
      expect(connected.length).toBeGreaterThanOrEqual(1);
      expect(connected.every(s => s.status === "connected")).toBe(true);
    });
  });

  describe("callTool", () => {
    it("should call tool on connected server", async () => {
      await manager.connectServer("echo", { type: "stdio", command: "echo" });

      const result = await manager.callTool("echo", "echo", { text: "hello" });
      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });

    it("should throw on unknown server", async () => {
      await expect(manager.callTool("unknown", "tool")).rejects.toThrow("No server registered");
    });

    it("should throw on disabled server", async () => {
      await manager.connectServer("disabled", { type: "stdio", command: "echo", disabled: true });
      await expect(manager.callTool("disabled", "tool")).rejects.toThrow("disabled");
    });

    it("should update lastActivity after tool call", async () => {
      await manager.connectServer("echo", { type: "stdio", command: "echo" });

      await manager.callTool("echo", "echo", {});

      const state = manager.getServerState("echo");
      expect(state!.lastActivity).toBeDefined();
    });
  });

  describe("readResource", () => {
    it("should read resource from connected server", async () => {
      await manager.connectServer("echo", { type: "stdio", command: "echo" });

      const result = await manager.readResource("echo", "file:///test.txt");
      expect(result.contents).toBeDefined();
    });

    it("should throw on unknown server", async () => {
      await expect(manager.readResource("unknown", "uri")).rejects.toThrow("No server registered");
    });
  });

  describe("event handlers", () => {
    it("should add and remove event handlers", () => {
      const handler = vi.fn();
      manager.addEventHandler(handler);
      manager.removeEventHandler(handler);
      expect(handler).not.toHaveBeenCalled();
    });
  });
});
