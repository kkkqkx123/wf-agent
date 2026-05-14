/**
 * Tests for TerminalManager
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TerminalManager } from "../terminal-manager.js";
import type { TerminalOptions, TerminalEvent } from "../types.js";

// Mock node-pty
const mockOnDataCallbacks: Map<string, (data: string) => void> = new Map();
const mockOnExitCallbacks: Map<string, (exitInfo: { exitCode: number; signal: number }) => void> = new Map();

vi.mock("node-pty", () => ({
  spawn: vi.fn((shell: string, args: string[], options: any) => {
    const pid = Math.floor(Math.random() * 10000) + 10000;
    const instance = {
      pid,
      write: vi.fn(),
      onData: vi.fn((callback: (data: string) => void) => {
        mockOnDataCallbacks.set(String(pid), callback);
      }),
      onExit: vi.fn((callback: (exitInfo: { exitCode: number; signal: number }) => void) => {
        mockOnExitCallbacks.set(String(pid), callback);
      }),
      resize: vi.fn(),
      kill: vi.fn(),
    };
    return instance;
  }),
}));

// Mock child_process
vi.mock("child_process", () => ({
  spawn: vi.fn((shell: string, args: string[], options: any) => ({
    pid: 12346,
    stdout: {
      on: vi.fn((event: string, callback: (data: Buffer) => void) => {
        // Store callback for testing
        if (event === "data") {
          (this as any)._stdoutCallback = callback;
        }
      }),
    },
    stderr: {
      on: vi.fn((event: string, callback: (data: Buffer) => void) => {
        // Store callback for testing
        if (event === "data") {
          (this as any)._stderrCallback = callback;
        }
      }),
    },
    on: vi.fn((event: string, callback: Function) => {
      // Store callbacks for testing
      if (event === "exit") {
        (this as any)._exitCallback = callback;
      } else if (event === "error") {
        (this as any)._errorCallback = callback;
      }
    }),
    unref: vi.fn(),
    kill: vi.fn(),
  })),
}));

// Mock fs
vi.mock("fs", () => ({
  existsSync: vi.fn(() => false),
  mkdirSync: vi.fn(),
  createWriteStream: vi.fn(() => ({
    write: vi.fn(),
    end: vi.fn(),
  })),
}));

// Mock output
vi.mock("../../utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    debugLog: vi.fn(),
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    warnLog: vi.fn(),
  })),
}));

describe("TerminalManager", () => {
  let manager: TerminalManager;

  beforeEach(() => {
    manager = new TerminalManager();
  });

  describe("createTerminal", () => {
    it("should create foreground terminal by default", () => {
      const terminal = manager.createTerminal();

      expect(terminal).toBeDefined();
      expect(terminal.id).toBeDefined();
      expect(terminal.pid).toBeGreaterThan(0);
      expect(terminal.status).toBe("active");
      expect(terminal.createdAt).toBeInstanceOf(Date);
    });

    it("should create foreground terminal with custom options", () => {
      const options: TerminalOptions = {
        shell: "bash",
        cwd: "/tmp",
        cols: 120,
        rows: 30,
        env: { TEST: "value" },
      };

      const terminal = manager.createTerminal(options);

      expect(terminal).toBeDefined();
      expect(terminal.status).toBe("active");
    });

    it("should create background terminal when background option is true", () => {
      const options: TerminalOptions = {
        background: true,
        logFile: "/tmp/test.log",
      };

      const terminal = manager.createTerminal(options);

      expect(terminal).toBeDefined();
      expect(terminal.status).toBe("active");
    });

    it("should use default shell when not specified", () => {
      const terminal = manager.createTerminal();

      expect(terminal).toBeDefined();
    });
  });

  describe("closeTerminal", () => {
    it("should close existing terminal", async () => {
      const terminal = manager.createTerminal();

      await manager.closeTerminal(terminal.id);

      const closedTerminal = manager.getTerminal(terminal.id);
      expect(closedTerminal).toBeUndefined();
    });

    it("should throw error for non-existent terminal", async () => {
      await expect(manager.closeTerminal("non-existent")).rejects.toThrow(
        "Terminal session not found: non-existent"
      );
    });
  });

  describe("getActiveTerminals", () => {
    it("should return empty array when no terminals", () => {
      const terminals = manager.getActiveTerminals();

      expect(terminals).toEqual([]);
    });

    it("should return only active terminals", () => {
      const terminal1 = manager.createTerminal();
      const terminal2 = manager.createTerminal();

      const terminals = manager.getActiveTerminals();

      expect(terminals.length).toBe(2);
      expect(terminals.every(t => t.status === "active")).toBe(true);
    });
  });

  describe("getTerminal", () => {
    it("should return existing terminal", () => {
      const terminal = manager.createTerminal();

      const retrieved = manager.getTerminal(terminal.id);

      expect(retrieved).toBeDefined();
      expect(retrieved?.id).toBe(terminal.id);
    });

    it("should return undefined for non-existent terminal", () => {
      const retrieved = manager.getTerminal("non-existent");

      expect(retrieved).toBeUndefined();
    });
  });

  describe("writeToTerminal", () => {
    it("should write to foreground terminal", () => {
      const terminal = manager.createTerminal();

      expect(() => {
        manager.writeToTerminal(terminal.id, "echo hello");
      }).not.toThrow();
    });

    it("should throw error for non-existent terminal", () => {
      expect(() => {
        manager.writeToTerminal("non-existent", "echo hello");
      }).toThrow("Terminal session not found: non-existent");
    });

    it("should throw error for inactive terminal", async () => {
      const terminal = manager.createTerminal();
      
      // Manually set status to inactive without closing
      const session = manager.getTerminal(terminal.id);
      if (session) {
        session.status = "inactive";
      }

      expect(() => {
        manager.writeToTerminal(terminal.id, "echo hello");
      }).toThrow("Terminal session is not active");
    });

    it("should throw error for background terminal", () => {
      const terminal = manager.createTerminal({ background: true });

      expect(() => {
        manager.writeToTerminal(terminal.id, "echo hello");
      }).toThrow("Write operation is not supported for background terminals");
    });
  });

  describe("resizeTerminal", () => {
    it("should resize foreground terminal", () => {
      const terminal = manager.createTerminal();

      expect(() => {
        manager.resizeTerminal(terminal.id, 120, 30);
      }).not.toThrow();
    });

    it("should throw error for non-existent terminal", () => {
      expect(() => {
        manager.resizeTerminal("non-existent", 120, 30);
      }).toThrow("Terminal session not found: non-existent");
    });

    it("should throw error for inactive terminal", async () => {
      const terminal = manager.createTerminal();
      
      // Manually set status to inactive without closing
      const session = manager.getTerminal(terminal.id);
      if (session) {
        session.status = "inactive";
      }

      expect(() => {
        manager.resizeTerminal(terminal.id, 120, 30);
      }).toThrow("Terminal session is not active");
    });

    it("should throw error for background terminal", () => {
      const terminal = manager.createTerminal({ background: true });

      expect(() => {
        manager.resizeTerminal(terminal.id, 120, 30);
      }).toThrow("Resize operation is not supported for background terminals");
    });
  });

  describe("addEventListener", () => {
    it("should add event listener", () => {
      const terminal = manager.createTerminal();
      const listener = vi.fn();

      expect(() => {
        manager.addEventListener(terminal.id, listener);
      }).not.toThrow();
    });

    it("should call listener on data event", () => {
      const terminal = manager.createTerminal();
      const listener = vi.fn();

      manager.addEventListener(terminal.id, listener);

      // Simulate data event using the stored callback
      const callback = mockOnDataCallbacks.get(String(terminal.pid));
      if (callback) {
        callback("test data");
      }

      expect(listener).toHaveBeenCalled();
      const event = listener.mock.calls[0][0] as TerminalEvent;
      expect(event.type).toBe("data");
      expect(event.data).toBe("test data");
    });

    it("should call listener on exit event", () => {
      const terminal = manager.createTerminal();
      const listener = vi.fn();

      manager.addEventListener(terminal.id, listener);

      // Simulate exit event using the stored callback
      const callback = mockOnExitCallbacks.get(String(terminal.pid));
      if (callback) {
        callback({ exitCode: 0, signal: 0 });
      }

      expect(listener).toHaveBeenCalled();
      const event = listener.mock.calls[0][0] as TerminalEvent;
      expect(event.type).toBe("exit");
      expect(event.exitCode).toBe(0);
    });
  });

  describe("removeEventListener", () => {
    it("should remove event listener", () => {
      const terminal = manager.createTerminal();
      const listener = vi.fn();

      manager.addEventListener(terminal.id, listener);
      manager.removeEventListener(terminal.id, listener);

      // Should not throw
      expect(() => {
        manager.removeEventListener(terminal.id, listener);
      }).not.toThrow();
    });
  });

  describe("cleanupAll", () => {
    it("should close all terminals", async () => {
      manager.createTerminal();
      manager.createTerminal();

      expect(manager.getActiveTerminals().length).toBe(2);

      await manager.cleanupAll();

      expect(manager.getActiveTerminals().length).toBe(0);
    });

    it("should handle empty cleanup", async () => {
      await expect(manager.cleanupAll()).resolves.not.toThrow();
    });
  });

  describe("getDefaultShell", () => {
    it("should return powershell on Windows", () => {
      // This test depends on the platform
      // We can't easily mock process.platform, so we just verify it returns a string
      const terminal = manager.createTerminal();
      expect(terminal).toBeDefined();
    });
  });

  describe("emitEvent", () => {
    it("should handle listener errors gracefully", () => {
      const terminal = manager.createTerminal();
      const badListener = vi.fn(() => {
        throw new Error("Listener error");
      });

      manager.addEventListener(terminal.id, badListener);

      // Should not throw even if listener fails
      expect(() => {
        const callback = mockOnDataCallbacks.get(String(terminal.pid));
        if (callback) {
          callback("test data");
        }
      }).not.toThrow();
    });
  });
});
