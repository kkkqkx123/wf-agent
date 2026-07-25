import type { Terminal } from "./terminal.js";

type RestoreFn = () => void;

export class TerminalGuard {
  private static instance: TerminalGuard | null = null;
  private terminal: Terminal | null = null;
  private restoreFns: RestoreFn[] = [];
  private armed = false;
  private suspended = false;
  private originalRaw = false;
  private onInput?: (data: string) => void;
  private onResize?: () => void;

  static getInstance(): TerminalGuard {
    if (!TerminalGuard.instance) {
      TerminalGuard.instance = new TerminalGuard();
    }
    return TerminalGuard.instance;
  }

  arm(terminal: Terminal, onInput?: (data: string) => void, onResize?: () => void): void {
    if (this.armed) return;
    this.terminal = terminal;
    this.onInput = onInput;
    this.onResize = onResize;
    this.armed = true;
    this.originalRaw = process.stdin.isRaw || false;

    this.restoreFns.push(this.registerSignalHandler("SIGINT", () => this.restoreAndExit(130)));
    this.restoreFns.push(this.registerSignalHandler("SIGTERM", () => this.restoreAndExit(143)));
    this.restoreFns.push(this.registerSignalHandler("SIGQUIT", () => this.restoreAndExit(131)));

    if (process.platform !== "win32") {
      this.restoreFns.push(this.registerSignalHandler("SIGTSTP", () => this.handleSuspend()));
      this.restoreFns.push(this.registerSignalHandler("SIGCONT", () => this.handleResume()));
    }

    this.restoreFns.push(this.registerProcessHandler("uncaughtException", (err) => {
      console.error("\nUncaught exception:", err);
      this.restoreAndExit(1);
    }));
    this.restoreFns.push(this.registerProcessHandler("unhandledRejection", (reason) => {
      console.error("\nUnhandled rejection:", reason);
    }));
    this.restoreFns.push(this.registerProcessHandler("beforeExit", () => this.disarm()));
  }

  disarm(): void {
    if (!this.armed) return;
    this.armed = false;
    for (const fn of this.restoreFns) {
      fn();
    }
    this.restoreFns = [];
    this.terminal = null;
  }

  private handleSuspend(): void {
    if (this.suspended || !this.terminal) return;
    this.suspended = true;
    this.terminal.showCursor();
    this.terminal.stop();
    const sigtstp = process.listeners("SIGTSTP").pop();
    if (sigtstp) {
      process.removeListener("SIGTSTP", sigtstp);
      process.kill(process.pid, "SIGSTOP");
    }
  }

  private handleResume(): void {
    if (!this.suspended) return;
    this.suspended = false;
    if (process.stdin.setRawMode) {
      process.stdin.setRawMode(true);
    }
    process.stdin.resume();
    if (this.terminal) {
      this.terminal.start(
        this.onInput ?? (() => {}),
        this.onResize ?? (() => {}),
      );
      this.terminal.hideCursor();
    }
  }

  private restoreAndExit(code: number): void {
    this.restoreTerminal();
    process.exit(code);
  }

  restoreTerminal(): void {
    if (process.stdin.setRawMode) {
      process.stdin.setRawMode(this.originalRaw);
    }
    process.stdin.pause();
    process.stdout.write("\x1b[?25h");
    process.stdout.write("\x1b[?2004l");
    process.stdout.write("\x1b[<u");
    process.stdout.write("\x1b[?2026l");
  }

  private registerSignalHandler(signal: NodeJS.Signals, handler: () => void): RestoreFn {
    process.on(signal, handler);
    return () => process.removeListener(signal, handler);
  }

  private registerProcessHandler(event: string, handler: (...args: any[]) => void): RestoreFn {
    process.on(event as any, handler);
    return () => process.removeListener(event as any, handler);
  }
}
