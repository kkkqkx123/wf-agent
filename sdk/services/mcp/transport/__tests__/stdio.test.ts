import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { StdioTransport } from "../stdio.js";
import type { TransportEventHandlers } from "../types.js";

// vi.mock is hoisted by vitest — define factory here
vi.mock("child_process", () => ({
  spawn: vi.fn(),
}));

import { spawn } from "child_process";

// Create a mock ChildProcess-like object
function createFakeChildProcess() {
  const eventListeners: Record<string, Array<(...args: unknown[]) => void>> = {};

  const fakeProcess: Record<string, unknown> = {
    pid: 12345,
    killed: false,
    stdin: {
      write: vi.fn((_data: string, cb?: (error?: Error | null) => void) => {
        if (cb) cb(null);
        return true;
      }),
      on: vi.fn(),
      end: vi.fn(),
    },
    stdout: {
      on: vi.fn((event: string, handler: (data: Buffer) => void) => {
        if (event === "data") {
          fakeProcess._stdoutHandler = handler;
        }
      }),
      pipe: vi.fn(),
    },
    stderr: {
      on: vi.fn((event: string, handler: (data: Buffer) => void) => {
        if (event === "data") {
          fakeProcess._stderrHandler = handler;
        }
      }),
      pipe: vi.fn(),
    },
    kill: vi.fn((_signal?: string | number) => {
      fakeProcess.killed = true;
      return true;
    }),
    on: vi.fn((event: string, handler: (...args: unknown[]) => void) => {
      if (!eventListeners[event]) eventListeners[event] = [];
      eventListeners[event].push(handler);
    }),
    removeListener: vi.fn(),
    emit: (event: string, ...args: unknown[]) => {
      const handlers = eventListeners[event] || [];
      handlers.forEach((h) => h(...args));
    },
    _stdoutHandler: null as ((data: Buffer) => void) | null,
    _stderrHandler: null as ((data: Buffer) => void) | null,
  };

  return fakeProcess;
}

describe("StdioTransport", () => {
  let transport: StdioTransport;
  let fakeProcess: ReturnType<typeof createFakeChildProcess>;

  beforeEach(() => {
    fakeProcess = createFakeChildProcess() as unknown as ReturnType<typeof createFakeChildProcess>;
    (spawn as ReturnType<typeof vi.fn>).mockReturnValue(fakeProcess);

    transport = new StdioTransport({ type: "stdio", command: "echo" });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("start", () => {
    it("should spawn process and mark as connected", async () => {
      await transport.start();
      expect(transport.isConnected).toBe(true);
    });

    it("should be idempotent if already started", async () => {
      await transport.start();
      await transport.start(); // Second call should be no-op
      expect(spawn).toHaveBeenCalledTimes(1);
    });

    it("should call onError on process error", async () => {
      const onError = vi.fn();
      transport.setHandlers({ onError } as TransportEventHandlers);

      await transport.start();
      // Emit error event
      (fakeProcess.emit as (event: string, ...args: unknown[]) => void)("error", new Error("Process crash"));
      expect(onError).toHaveBeenCalled();
    });

    it("should parse JSON from stdout", async () => {
      const onData = vi.fn();
      transport.setHandlers({ onData } as TransportEventHandlers);

      await transport.start();
      // Simulate stdout data
      if (fakeProcess._stdoutHandler) {
        fakeProcess._stdoutHandler(Buffer.from('{"jsonrpc":"2.0","id":1,"result":"ok"}'));
      }
      expect(onData).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 1, result: "ok" });
    });
  });

  describe("send", () => {
    it("should write JSON to stdin", async () => {
      await transport.start();
      const stdinWrite = (fakeProcess.stdin as { write: ReturnType<typeof vi.fn> }).write;
      await transport.send({ jsonrpc: "2.0", method: "ping" });

      expect(stdinWrite).toHaveBeenCalledWith(
        '{"jsonrpc":"2.0","method":"ping"}\n',
        expect.any(Function),
      );
    });

    it("should throw if not connected", async () => {
      await expect(transport.send({})).rejects.toThrow("Transport not connected");
    });
  });

  describe("close", () => {
    it("should close the process gracefully", async () => {
      await transport.start();

      const closePromise = transport.close();

      // Simulate process exit event (happens after kill)
      setImmediate(() => {
        (fakeProcess.emit as (event: string, ...args: unknown[]) => void)("exit", 0, "SIGTERM");
      });

      await closePromise;
      expect(transport.isConnected).toBe(false);
    });
  });

  describe("getStderr", () => {
    it("should return stderr stream after start", async () => {
      await transport.start();
      expect(transport.getStderr()).toBe((fakeProcess as Record<string, unknown>).stderr);
    });

    it("should return null before start", () => {
      const t = new StdioTransport({ type: "stdio", command: "echo" });
      expect(t.getStderr()).toBeNull();
    });
  });
});
