import fs from "node:fs";

export class ConfigWatcher {
  private watching = false;
  private paths: string[] = [];
  private mtimes: Map<string, number> = new Map();
  private pollTimer?: ReturnType<typeof setInterval>;
  private callback?: () => void;
  private readonly POLL_INTERVAL = 500;

  watch(path: string, cb: () => void): void {
    if (!this.paths.includes(path)) {
      this.paths.push(path);
    }
    this.callback = cb;

    if (!this.watching) {
      this.watching = true;
      this.pollTimer = setInterval(() => this.poll(), this.POLL_INTERVAL);
    }
  }

  unwatch(): void {
    this.watching = false;
    this.paths = [];
    this.mtimes.clear();
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = undefined;
    }
    this.callback = undefined;
  }

  private poll(): void {
    if (!this.callback) return;

    for (const configPath of this.paths) {
      try {
        const stat = fs.statSync(configPath);
        const prev = this.mtimes.get(configPath) ?? 0;
        if (stat.mtimeMs > prev) {
          this.mtimes.set(configPath, stat.mtimeMs);
          this.callback();
        }
      } catch {
        // Config file not found or not readable — skip silently
      }
    }
  }
}
