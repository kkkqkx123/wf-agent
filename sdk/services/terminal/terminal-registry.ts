/**
 * Terminal Registry
 * 
 * Singleton registry for managing terminal sessions with:
 * - Session lifecycle management
 * - Intelligent session reuse
 * - Task-based session association
 */

import type {
  TerminalSession,
  TerminalSessionOptions,
  ShellType,
} from "./types.js";

/**
 * Terminal Registry (Singleton)
 * 
 * Manages all terminal sessions and provides intelligent session reuse.
 */
export class TerminalRegistry {
  private static instance: TerminalRegistry;

  private sessions: Map<string, TerminalSession> = new Map();
  private nextSessionId = 1;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static getInstance(): TerminalRegistry {
    if (!TerminalRegistry.instance) {
      TerminalRegistry.instance = new TerminalRegistry();
    }
    return TerminalRegistry.instance;
  }

  /**
   * Create a new session
   */
  createSession(
    shellType: ShellType,
    cwd: string,
    options?: TerminalSessionOptions
  ): TerminalSession {
    const sessionId = `session-${this.nextSessionId++}`;
    const now = Date.now();

    const session: TerminalSession = {
      sessionId,
      shellType,
      cwd,
      env: options?.env ?? {},
      status: "idle",
      createdAt: now,
      lastActiveAt: now,
      taskId: options?.taskId,
      outputLines: [],
      lastReadIndex: 0,
    };

    this.sessions.set(sessionId, session);

    return session;
  }

  /**
   * Register an existing session
   */
  register(session: TerminalSession): void {
    this.sessions.set(session.sessionId, session);
  }

  /**
   * Get a session by ID
   */
  get(sessionId: string): TerminalSession | undefined {
    const session = this.sessions.get(sessionId);

    // Check if session is terminated
    if (session?.status === "terminated") {
      this.sessions.delete(sessionId);
      return undefined;
    }

    return session;
  }

  /**
   * Get all sessions
   */
  getAll(): TerminalSession[] {
    // Filter out terminated sessions
    const activeSessions: TerminalSession[] = [];

    for (const [id, session] of this.sessions) {
      if (session.status === "terminated") {
        this.sessions.delete(id);
      } else {
        activeSessions.push(session);
      }
    }

    return activeSessions;
  }

  /**
   * Find an available session matching criteria
   * 
   * Priority:
   * 1. Session with matching taskId and cwd
   * 2. Session with matching cwd (no taskId or different taskId)
   * 3. No match found
   */
  findAvailable(cwd: string, taskId?: string): TerminalSession | undefined {
    const sessions = this.getAll();

    // First priority: Find a session with matching taskId and cwd
    if (taskId) {
      const matchByTaskAndCwd = sessions.find(
        (s) =>
          s.status === "idle" &&
          s.taskId === taskId &&
          this.arePathsEqual(s.cwd, cwd)
      );
      if (matchByTaskAndCwd) {
        return matchByTaskAndCwd;
      }
    }

    // Second priority: Find a session with matching cwd
    const matchByCwd = sessions.find(
      (s) => s.status === "idle" && this.arePathsEqual(s.cwd, cwd)
    );

    return matchByCwd;
  }

  /**
   * Update session status
   */
  updateStatus(sessionId: string, status: TerminalSession["status"]): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.status = status;
      session.lastActiveAt = Date.now();
    }
  }

  /**
   * Update session working directory
   */
  updateCwd(sessionId: string, cwd: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.cwd = cwd;
      session.lastActiveAt = Date.now();
    }
  }

  /**
   * Update session task ID
   */
  updateTaskId(sessionId: string, taskId: string | undefined): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.taskId = taskId;
      session.lastActiveAt = Date.now();
    }
  }

  /**
   * Add output line to session
   */
  addOutput(sessionId: string, line: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.outputLines.push(line);
    }
  }

  /**
   * Get output lines from session
   */
  getOutput(sessionId: string, fromIndex?: number): string[] {
    const session = this.sessions.get(sessionId);
    if (!session) {
      return [];
    }

    const start = fromIndex ?? session.lastReadIndex;
    const lines = session.outputLines.slice(start);

    return lines;
  }

  /**
   * Mark output as read
   */
  markOutputRead(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.lastReadIndex = session.outputLines.length;
    }
  }

  /**
   * Release a session (mark as available for reuse)
   */
  release(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.status = "idle";
      session.taskId = undefined;
      session.lastActiveAt = Date.now();
    }
  }

  /**
   * Terminate a session
   */
  terminate(sessionId: string): TerminalSession | undefined {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.status = "terminated";
      this.sessions.delete(sessionId);
      return session;
    }
    return undefined;
  }

  /**
   * Release all sessions for a task
   */
  releaseForTask(taskId: string): void {
    for (const session of this.sessions.values()) {
      if (session.taskId === taskId) {
        session.taskId = undefined;
        session.status = "idle";
        session.lastActiveAt = Date.now();
      }
    }
  }

  /**
   * Terminate all sessions for a task
   */
  terminateForTask(taskId: string): TerminalSession[] {
    const terminated: TerminalSession[] = [];

    for (const [id, session] of this.sessions) {
      if (session.taskId === taskId) {
        session.status = "terminated";
        terminated.push(session);
        this.sessions.delete(id);
      }
    }

    return terminated;
  }

  /**
   * Get session count
   */
  getCount(): number {
    return this.getAll().length;
  }

  /**
   * Get sessions by status
   */
  getByStatus(status: TerminalSession["status"]): TerminalSession[] {
    return this.getAll().filter((s) => s.status === status);
  }

  /**
   * Get sessions by task ID
   */
  getByTaskId(taskId: string): TerminalSession[] {
    return this.getAll().filter((s) => s.taskId === taskId);
  }

  /**
   * Cleanup all sessions
   */
  cleanup(): void {
    this.sessions.clear();
    this.nextSessionId = 1;
  }

  /**
   * Compare two paths for equality
   * 
   * Handles platform-specific path comparison.
   */
  private arePathsEqual(path1: string, path2: string): boolean {
    // Normalize paths
    const normalized1 = this.normalizePath(path1);
    const normalized2 = this.normalizePath(path2);

    return normalized1 === normalized2;
  }

  /**
   * Normalize a path for comparison
   */
  private normalizePath(path: string): string {
    // Remove trailing slashes
    let normalized = path.replace(/[\\/]+$/, "");

    // On Windows, normalize case
    if (process.platform === "win32") {
      normalized = normalized.toLowerCase();
    }

    return normalized;
  }
}

/**
 * Default registry instance
 */
export const terminalRegistry = TerminalRegistry.getInstance();
