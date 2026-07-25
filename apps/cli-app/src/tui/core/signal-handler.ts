type SignalCallback = () => void;

export class SignalHandler {
  private suspendCbs: SignalCallback[] = [];
  private resumeCbs: SignalCallback[] = [];
  private interruptCbs: SignalCallback[] = [];

  private suspended = false;
  private interruptDelay = 0;
  private armed = false;
  private cleanupFns: (() => void)[] = [];

  arm(): void {
    if (this.armed) return;
    this.armed = true;

    this.cleanupFns.push(this.registerSignal("SIGINT", () => {
      if (this.interruptDelay > 0) {
        const elapsed = Date.now() - this.interruptDelay;
        if (elapsed < 500) return;
      }
      this.interruptDelay = Date.now();
      for (const cb of this.interruptCbs) cb();
    }));

    if (process.platform !== "win32") {
      this.cleanupFns.push(this.registerSignal("SIGTSTP", () => {
        if (this.suspended) return;
        this.suspended = true;
        for (const cb of this.suspendCbs) cb();
        process.kill(process.pid, "SIGSTOP");
      }));

      this.cleanupFns.push(this.registerSignal("SIGCONT", () => {
        if (!this.suspended) return;
        this.suspended = false;
        for (const cb of this.resumeCbs) cb();
      }));
    }
  }

  disarm(): void {
    if (!this.armed) return;
    this.armed = false;
    for (const fn of this.cleanupFns) fn();
    this.cleanupFns = [];
    this.suspendCbs = [];
    this.resumeCbs = [];
    this.interruptCbs = [];
  }

  onSuspend(cb: SignalCallback): void {
    this.suspendCbs.push(cb);
  }

  onResume(cb: SignalCallback): void {
    this.resumeCbs.push(cb);
  }

  onInterrupt(cb: SignalCallback): void {
    this.interruptCbs.push(cb);
  }

  private registerSignal(signal: string, handler: () => void): () => void {
    process.on(signal as NodeJS.Signals, handler);
    return () => { process.removeListener(signal as NodeJS.Signals, handler); };
  }
}
