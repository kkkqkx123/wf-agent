/**
 * Terminal Manager
 * Responsible for creating and managing pseudo-terminal sessions.
 */

import * as pty from "node-pty";
import { spawn } from "child_process";
import { randomUUID } from "crypto";
import * as fs from "fs";
import * as path from "path";
import { getOutput } from "../utils/output.js";
import type { TerminalOptions, TerminalSession, TerminalEvent } from "./types.js";

const output = getOutput();

/**
 * Terminal Manager
 * Responsible for creating and managing pseudo-terminal sessions.
 */
export class TerminalManager {
  /** Terminal Session Mapping Table */
  private sessions: Map<string, TerminalSession> = new Map();
  /** Event Listener Mapping Table */
  private eventListeners: Map<string, Set<(event: TerminalEvent) => void>> = new Map();

  /**
   * Create new terminal session
   * @param options Terminal configuration options
   * @returns Terminal session object
   */
  createTerminal(options: TerminalOptions = {}): TerminalSession {
    const sessionId = randomUUID();
    const shell = options.shell || this.getDefaultShell();

    try {
      if (options.background) {
        // Background execution mode: Using `child_process.spawn`
        return this.createBackgroundTerminal(sessionId, shell, options);
      } else {
        // Front-end runtime mode: Using node-pty
        return this.createForegroundTerminal(sessionId, shell, options);
      }
    } catch (error) {
      output.errorLog(
        `Failed to create terminal session: ${error instanceof Error ? error.message : String(error)}`,
      );
      throw error;
    }
  }

  /**
   * Create foreground terminal (using node-pty)
   */
  private createForegroundTerminal(
    sessionId: string,
    shell: string,
    options: TerminalOptions,
  ): TerminalSession {
    // Create a pseudo-terminal
    const ptyProcess = pty.spawn(shell, [], {
      name: "xterm-color",
      cols: options.cols || 80,
      rows: options.rows || 24,
      cwd: options.cwd || process.cwd(),
      env: { ...process.env, ...options.env },
    });

    const session: TerminalSession = {
      id: sessionId,
      pty: ptyProcess,
      pid: ptyProcess.pid,
      createdAt: new Date(),
      status: "active",
    };

    this.sessions.set(sessionId, session);
    output.infoLog(`Foreground terminal created: ${sessionId}, PID: ${ptyProcess.pid}`);

    // Listening to terminal data output
    ptyProcess.onData((data: string) => {
      this.emitEvent(sessionId, {
        type: "data",
        data,
      });
    });

    // Listen for the terminal exit event.
    ptyProcess.onExit(({ exitCode, signal }) => {
      output.infoLog(
        `Foreground terminal exited: ${sessionId}, exitCode: ${exitCode}, signal: ${signal}`,
      );
      session.status = "closed";
      this.emitEvent(sessionId, {
        type: "exit",
        exitCode,
        signal,
      });
    });

    return session;
  }

  /**
   * Create background terminal (using child_process.spawn)
   */
  private createBackgroundTerminal(
    sessionId: string,
    shell: string,
    options: TerminalOptions,
  ): TerminalSession {
    const logFile = options.logFile || `logs/task-${sessionId}.log`;

    // Create a background process
    const childProcess = spawn(shell, [], {
      cwd: options.cwd || process.cwd(),
      env: { ...process.env, ...options.env },
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
    });

    const session: TerminalSession = {
      id: sessionId,
      pty: childProcess,
      pid: childProcess.pid || 0,
      createdAt: new Date(),
      status: "active",
    };

    this.sessions.set(sessionId, session);
    output.infoLog(
      `Background terminal created: ${sessionId}, PID: ${childProcess.pid}, log: ${logFile}`,
    );

    // Redirect the output to a log file.
    const logDir = path.dirname(logFile);

    // Ensure the log directory exists.
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }

    const logStream = fs.createWriteStream(logFile, { flags: "a" });

    // Listen to the standard output.
    childProcess.stdout?.on("data", (data: Buffer) => {
      const content = data.toString();
      logStream.write(content);
      this.emitEvent(sessionId, {
        type: "data",
        data: content,
      });
    });

    // Listen for standard errors
    childProcess.stderr?.on("data", (data: Buffer) => {
      const err = data.toString();
      logStream.write(`[ERROR] ${err}`);
      this.emitEvent(sessionId, {
        type: "error",
        error: new Error(err),
      });
    });

    // Listening for process exit events
    childProcess.on("exit", (code: number | null, signal: number | null) => {
      output.infoLog(
        `Background terminal exited: ${sessionId}, exitCode: ${code}, signal: ${signal}`,
      );
      session.status = "closed";
      logStream.end();
      this.emitEvent(sessionId, {
        type: "exit",
        exitCode: code || undefined,
        signal: signal || undefined,
      });
    });

    // Listening process error
    childProcess.on("error", (error: Error) => {
      output.errorLog(`Background terminal error: ${sessionId}, error: ${error.message}`);
      session.status = "closed";
      logStream.write(`[ERROR] ${error.message}\n`);
      logStream.end();
      this.emitEvent(sessionId, {
        type: "error",
        error,
      });
    });

    // Separate the processes so that they run in the background.
    childProcess.unref();

    return session;
  }

  /**
   * Close specified terminal
   * @param sessionId Terminal session ID
   */
  async closeTerminal(sessionId: string): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Terminal session not found: ${sessionId}`);
    }

    if (session.status !== "closed") {
      try {
        session.pty.kill();
        session.status = "closed";
        output.infoLog(`Terminal session closed: ${sessionId}`);
      } catch (error) {
        output.errorLog(
          `Failed to close terminal session: ${error instanceof Error ? error.message : String(error)}`,
        );
        throw error;
      }
    }

    this.sessions.delete(sessionId);
    this.eventListeners.delete(sessionId);
  }

  /**
   * Get all active terminals
   * @returns List of active terminals
   */
  getActiveTerminals(): TerminalSession[] {
    return Array.from(this.sessions.values()).filter(session => session.status === "active");
  }

  /**
   * Get specified terminal
   * @param sessionId Terminal session ID
   * @returns Terminal session object
   */
  getTerminal(sessionId: string): TerminalSession | undefined {
    return this.sessions.get(sessionId);
  }

  /**
   * Write data to terminal
   * @param sessionId Terminal session ID
   * @param data Data to write
   */
  writeToTerminal(sessionId: string, data: string): void {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Terminal session not found: ${sessionId}`);
    }

    if (session.status !== "active") {
      throw new Error(`Terminal session is not active: ${sessionId}`);
    }

    session.pty.write(data);
  }

  /**
   * Resize terminal
   * @param sessionId Terminal session ID
   * @param cols Number of columns
   * @param rows Number of rows
   */
  resizeTerminal(sessionId: string, cols: number, rows: number): void {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Terminal session not found: ${sessionId}`);
    }

    if (session.status !== "active") {
      throw new Error(`Terminal session is not active: ${sessionId}`);
    }

    session.pty.resize(cols, rows);
    output.debugLog(`Terminal resized: ${sessionId}, cols: ${cols}, rows: ${rows}`);
  }

  /**
   * Add event listener
   * @param sessionId Terminal session ID
   * @param listener Event listener
   */
  addEventListener(sessionId: string, listener: (event: TerminalEvent) => void): void {
    if (!this.eventListeners.has(sessionId)) {
      this.eventListeners.set(sessionId, new Set());
    }
    this.eventListeners.get(sessionId)!.add(listener);
  }

  /**
   * Remove event listener
   * @param sessionId Terminal session ID
   * @param listener Event listener
   */
  removeEventListener(sessionId: string, listener: (event: TerminalEvent) => void): void {
    const listeners = this.eventListeners.get(sessionId);
    if (listeners) {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.eventListeners.delete(sessionId);
      }
    }
  }

  /**
   * Cleanup all terminals
   */
  async cleanupAll(): Promise<void> {
    const sessionIds = Array.from(this.sessions.keys());
    const promises = sessionIds.map(sessionId => this.closeTerminal(sessionId));
    await Promise.all(promises);
    output.infoLog("All terminal sessions have been cleared");
  }

  /**
   * Get default Shell
   * @returns Shell path
   */
  private getDefaultShell(): string {
    if (process.platform === "win32") {
      return "powershell.exe";
    }
    return process.env["SHELL"] || "bash";
  }

  /**
   * Emit event
   * @param sessionId Terminal session ID
   * @param event Terminal event
   */
  private emitEvent(sessionId: string, event: TerminalEvent): void {
    const listeners = this.eventListeners.get(sessionId);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(event);
        } catch (error) {
          output.errorLog(
            `Event listener execution failed: ${error instanceof Error ? error.message : String(error)}`,
          );
        }
      });
    }
  }
}
