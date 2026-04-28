/**
 * Communication Bridge
 * Responsible for communication between the main process and terminal processes.
 */

import { Subject, Observable, Subscription } from "rxjs";
import { getOutput } from "../utils/output.js";
import type { BridgeMessage, TerminalSession } from "./types.js";

const output = getOutput();

/**
 * Communication Bridge
 * Responsible for communication between the main process and terminal processes.
 */
export class CommunicationBridge {
  /** Message Queue Mapping Table */
  private messageQueues: Map<string, Subject<BridgeMessage>> = new Map();
  /** Subscription Mapping Table */
  private subscriptions: Map<string, Subscription[]> = new Map();
  /** Global Message Broadcaster */
  private globalBroadcaster: Subject<BridgeMessage> = new Subject();

  /**
   * Send message to specified terminal
   * @param sessionId Terminal session ID
   * @param message Message object
   */
  sendToTerminal(sessionId: string, message: BridgeMessage): void {
    const queue = this.messageQueues.get(sessionId);
    if (queue) {
      queue.next(message);
      output.debugLog(`Message sent to terminal: ${sessionId}, type: ${message.type}`);
    } else {
      output.warnLog(`Terminal not found: ${sessionId}`);
    }
  }

  /**
   * Broadcast message to all terminals
   * @param message Message object
   */
  broadcast(message: BridgeMessage): void {
    this.globalBroadcaster.next(message);
    this.messageQueues.forEach((queue, sessionId) => {
      queue.next(message);
      output.debugLog(`Message broadcast to terminal: ${sessionId}, type: ${message.type}`);
    });
  }

  /**
   * Receive message from terminal
   * @param sessionId Terminal session ID
   * @returns Message stream
   */
  receiveFromTerminal(sessionId: string): Observable<BridgeMessage> {
    if (!this.messageQueues.has(sessionId)) {
      this.messageQueues.set(sessionId, new Subject<BridgeMessage>());
    }
    return this.messageQueues.get(sessionId)!.asObservable();
  }

  /**
   * Subscribe to global messages
   * @returns Global message stream
   */
  subscribeGlobal(): Observable<BridgeMessage> {
    return this.globalBroadcaster.asObservable();
  }

  /**
   * Sync task status
   * @param taskId Task ID
   * @param status Task status
   */
  syncTaskStatus(taskId: string, status: any): void {
    const message: BridgeMessage = {
      type: "status",
      payload: { taskId, status },
      timestamp: new Date(),
    };

    this.broadcast(message);
  }

  /**
   * Send output message
   * @param sessionId Terminal session ID
   * @param content Output content
   */
  sendOutput(sessionId: string, content: string): void {
    const message: BridgeMessage = {
      type: "output",
      payload: { output: content },
      timestamp: new Date(),
    };

    this.sendToTerminal(sessionId, message);
  }

  /**
   * Send error message
   * @param sessionId Terminal session ID
   * @param error Error message
   */
  sendError(sessionId: string, error: string): void {
    const message: BridgeMessage = {
      type: "error",
      payload: { error },
      timestamp: new Date(),
    };

    this.sendToTerminal(sessionId, message);
  }

  /**
   * Send command message
   * @param sessionId Terminal session ID
   * @param command Command object
   */
  sendCommand(sessionId: string, command: any): void {
    const message: BridgeMessage = {
      type: "command",
      payload: command,
      timestamp: new Date(),
    };

    this.sendToTerminal(sessionId, message);
  }

  /**
   * Subscribe to messages from specified terminal
   * @param sessionId Terminal session ID
   * @param callback Callback function
   * @returns Subscription object
   */
  subscribe(sessionId: string, callback: (message: BridgeMessage) => void): Subscription {
    const observable = this.receiveFromTerminal(sessionId);
    const subscription = observable.subscribe(callback);

    // Save the subscription for future cleanup.
    if (!this.subscriptions.has(sessionId)) {
      this.subscriptions.set(sessionId, []);
    }
    this.subscriptions.get(sessionId)!.push(subscription);

    return subscription;
  }

  /**
   * Unsubscribe from specified terminal
   * @param sessionId Terminal session ID
   */
  unsubscribe(sessionId: string): void {
    const subs = this.subscriptions.get(sessionId);
    if (subs) {
      subs.forEach(sub => sub.unsubscribe());
      this.subscriptions.delete(sessionId);
      output.debugLog(`Unsubscribed from terminal: ${sessionId}`);
    }
  }

  /**
   * Cleanup message queue for specified terminal
   * @param sessionId Terminal session ID
   */
  cleanup(sessionId: string): void {
    // Unsubscribe from all subscriptions.
    this.unsubscribe(sessionId);

    // Complete the message queue setup.
    const queue = this.messageQueues.get(sessionId);
    if (queue) {
      queue.complete();
      this.messageQueues.delete(sessionId);
      output.debugLog(`Cleaned up message queue for terminal: ${sessionId}`);
    }
  }

  /**
   * Cleanup all message queues and subscriptions
   */
  cleanupAll(): void {
    // Clean up all message queues.
    this.messageQueues.forEach(queue => queue.complete());
    this.messageQueues.clear();

    // Cancel all subscriptions
    this.subscriptions.forEach(subs => {
      subs.forEach(sub => sub.unsubscribe());
    });
    this.subscriptions.clear();

    // Complete the global broadcaster.
    this.globalBroadcaster.complete();

    output.infoLog("All message queues and subscriptions have been cleaned up");
  }

  /**
   * Get the number of active terminals
   * @returns Number of active terminals
   */
  getActiveTerminalCount(): number {
    return this.messageQueues.size;
  }

  /**
   * Check if terminal exists
   * @param sessionId Terminal session ID
   * @returns Whether it exists
   */
  hasTerminal(sessionId: string): boolean {
    return this.messageQueues.has(sessionId);
  }

  /**
   * Get all active terminal IDs
   * @returns List of terminal IDs
   */
  getActiveTerminalIds(): string[] {
    return Array.from(this.messageQueues.keys());
  }
}
