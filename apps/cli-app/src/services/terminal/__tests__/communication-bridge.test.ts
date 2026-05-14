/**
 * Tests for CommunicationBridge
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { CommunicationBridge } from "../communication-bridge.js";
import type { BridgeMessage } from "../types.js";

// Mock the output module
vi.mock("../../utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    debugLog: vi.fn(),
    warnLog: vi.fn(),
    infoLog: vi.fn(),
    errorLog: vi.fn(),
  })),
}));

describe("CommunicationBridge", () => {
  let bridge: CommunicationBridge;

  beforeEach(() => {
    bridge = new CommunicationBridge();
  });

  afterEach(() => {
    bridge.cleanupAll();
  });

  describe("sendToTerminal", () => {
    it("should send message to existing terminal", () => {
      const sessionId = "test-session";
      const message: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      // Create a queue for the session first
      bridge.receiveFromTerminal(sessionId);
      
      // Should not throw
      expect(() => bridge.sendToTerminal(sessionId, message)).not.toThrow();
    });

    it("should warn when sending to non-existent terminal", () => {
      const sessionId = "non-existent";
      const message: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      // Should not throw but should log warning
      expect(() => bridge.sendToTerminal(sessionId, message)).not.toThrow();
    });
  });

  describe("broadcast", () => {
    it("should broadcast message to all terminals", () => {
      const message: BridgeMessage = {
        type: "status",
        payload: { status: "running" },
        timestamp: new Date(),
      };

      // Create some terminals
      bridge.receiveFromTerminal("session-1");
      bridge.receiveFromTerminal("session-2");

      // Should not throw
      expect(() => bridge.broadcast(message)).not.toThrow();
    });

    it("should broadcast to global broadcaster", () => {
      const message: BridgeMessage = {
        type: "status",
        payload: { status: "running" },
        timestamp: new Date(),
      };

      let received = false;
      const subscription = bridge.subscribeGlobal().subscribe((msg: BridgeMessage) => {
        received = true;
      });

      bridge.broadcast(message);
      
      expect(received).toBe(true);
      subscription.unsubscribe();
    });
  });

  describe("receiveFromTerminal", () => {
    it("should create observable for new terminal", () => {
      const observable = bridge.receiveFromTerminal("new-session");
      
      expect(observable).toBeDefined();
      expect(bridge.hasTerminal("new-session")).toBe(true);
    });

    it("should return same observable for existing terminal", () => {
      bridge.receiveFromTerminal("existing-session");
      const obs1 = bridge.receiveFromTerminal("existing-session");
      const obs2 = bridge.receiveFromTerminal("existing-session");
      
      // Both should be observables from the same subject
      expect(obs1).toBeDefined();
      expect(obs2).toBeDefined();
    });

    it("should emit messages to subscribers", () => {
      const sessionId = "test-session";
      const message: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      let receivedMessage: BridgeMessage | null = null;
      const subscription = bridge.receiveFromTerminal(sessionId).subscribe((msg: BridgeMessage) => {
        receivedMessage = msg;
      });

      bridge.sendToTerminal(sessionId, message);
      
      expect(receivedMessage).toEqual(message);
      subscription.unsubscribe();
    });
  });

  describe("subscribeGlobal", () => {
    it("should return global message observable", () => {
      const observable = bridge.subscribeGlobal();
      
      expect(observable).toBeDefined();
    });

    it("should receive all broadcasted messages", () => {
      const messages: BridgeMessage[] = [];
      const subscription = bridge.subscribeGlobal().subscribe((msg: BridgeMessage) => {
        messages.push(msg);
      });

      const msg1: BridgeMessage = {
        type: "status",
        payload: { status: "running" },
        timestamp: new Date(),
      };
      const msg2: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      bridge.broadcast(msg1);
      bridge.broadcast(msg2);
      
      expect(messages.length).toBe(2);
      expect(messages[0].type).toBe("status");
      expect(messages[1].type).toBe("output");
      
      subscription.unsubscribe();
    });
  });

  describe("syncTaskStatus", () => {
    it("should broadcast task status", () => {
      const taskId = "task-123";
      const status = { progress: 50 };

      let received = false;
      const subscription = bridge.subscribeGlobal().subscribe((msg: BridgeMessage) => {
        if (msg.type === "status") {
          received = true;
          expect(msg.payload.taskId).toBe(taskId);
          expect(msg.payload.status).toEqual(status);
        }
      });

      bridge.syncTaskStatus(taskId, status);
      
      expect(received).toBe(true);
      subscription.unsubscribe();
    });
  });

  describe("sendOutput", () => {
    it("should send output message to terminal", () => {
      const sessionId = "test-session";
      const content = "test output";

      // Create queue first
      bridge.receiveFromTerminal(sessionId);

      let receivedMessage: BridgeMessage | null = null;
      const subscription = bridge.receiveFromTerminal(sessionId).subscribe((msg: BridgeMessage) => {
        receivedMessage = msg;
      });

      bridge.sendOutput(sessionId, content);
      
      expect(receivedMessage).not.toBeNull();
      const msg = receivedMessage as unknown as BridgeMessage;
      expect(msg.type).toBe("output");
      expect((msg.payload as any).output).toBe(content);
      
      subscription.unsubscribe();
    });
  });

  describe("sendError", () => {
    it("should send error message to terminal", () => {
      const sessionId = "test-session";
      const error = "test error";

      // Create queue first
      bridge.receiveFromTerminal(sessionId);

      let receivedMessage: BridgeMessage | null = null;
      const subscription = bridge.receiveFromTerminal(sessionId).subscribe((msg: BridgeMessage) => {
        receivedMessage = msg;
      });

      bridge.sendError(sessionId, error);
      
      expect(receivedMessage).not.toBeNull();
      const msg = receivedMessage as unknown as BridgeMessage;
      expect(msg.type).toBe("error");
      expect((msg.payload as any).error).toBe(error);
      
      subscription.unsubscribe();
    });
  });

  describe("sendCommand", () => {
    it("should send command message to terminal", () => {
      const sessionId = "test-session";
      const command = { cmd: "ls", args: ["-la"] };

      // Create queue first
      bridge.receiveFromTerminal(sessionId);

      let receivedMessage: BridgeMessage | null = null;
      const subscription = bridge.receiveFromTerminal(sessionId).subscribe((msg: BridgeMessage) => {
        receivedMessage = msg;
      });

      bridge.sendCommand(sessionId, command);
      
      expect(receivedMessage).not.toBeNull();
      const msg = receivedMessage as unknown as BridgeMessage;
      expect(msg.type).toBe("command");
      expect(msg.payload).toEqual(command);
      
      subscription.unsubscribe();
    });
  });

  describe("subscribe and unsubscribe", () => {
    it("should subscribe to terminal messages", () => {
      const sessionId = "test-session";
      const callback = vi.fn();

      const subscription = bridge.subscribe(sessionId, callback);
      
      expect(subscription).toBeDefined();
      
      const message: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      bridge.sendToTerminal(sessionId, message);
      
      expect(callback).toHaveBeenCalledWith(message);
      subscription.unsubscribe();
    });

    it("should unsubscribe from terminal messages", () => {
      const sessionId = "test-session";
      const callback = vi.fn();

      bridge.subscribe(sessionId, callback);
      bridge.unsubscribe(sessionId);

      const message: BridgeMessage = {
        type: "output",
        payload: { output: "test" },
        timestamp: new Date(),
      };

      bridge.sendToTerminal(sessionId, message);
      
      expect(callback).not.toHaveBeenCalled();
    });
  });

  describe("cleanup", () => {
    it("should cleanup specific terminal", () => {
      const sessionId = "test-session";
      bridge.receiveFromTerminal(sessionId);
      
      expect(bridge.hasTerminal(sessionId)).toBe(true);
      
      bridge.cleanup(sessionId);
      
      expect(bridge.hasTerminal(sessionId)).toBe(false);
    });

    it("should cleanup all terminals", () => {
      bridge.receiveFromTerminal("session-1");
      bridge.receiveFromTerminal("session-2");
      
      expect(bridge.getActiveTerminalCount()).toBe(2);
      
      bridge.cleanupAll();
      
      expect(bridge.getActiveTerminalCount()).toBe(0);
    });
  });

  describe("utility methods", () => {
    it("should get active terminal count", () => {
      expect(bridge.getActiveTerminalCount()).toBe(0);
      
      bridge.receiveFromTerminal("session-1");
      expect(bridge.getActiveTerminalCount()).toBe(1);
      
      bridge.receiveFromTerminal("session-2");
      expect(bridge.getActiveTerminalCount()).toBe(2);
    });

    it("should check if terminal exists", () => {
      expect(bridge.hasTerminal("non-existent")).toBe(false);
      
      bridge.receiveFromTerminal("session-1");
      expect(bridge.hasTerminal("session-1")).toBe(true);
    });

    it("should get all active terminal IDs", () => {
      expect(bridge.getActiveTerminalIds()).toEqual([]);
      
      bridge.receiveFromTerminal("session-1");
      bridge.receiveFromTerminal("session-2");
      
      const ids = bridge.getActiveTerminalIds();
      expect(ids.length).toBe(2);
      expect(ids).toContain("session-1");
      expect(ids).toContain("session-2");
    });
  });
});
