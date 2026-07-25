export class InputBuffer {
  private queue: string[] = [];
  private _paused = false;
  private pending: string[] = [];

  push(input: string): void {
    if (this._paused) {
      this.pending.push(input);
    } else {
      this.queue.push(input);
    }
  }

  shift(): string | null {
    return this.queue.shift() ?? null;
  }

  flush(): string[] {
    const entries = this.queue;
    this.queue = [];
    return entries;
  }

  clear(): void {
    this.queue = [];
    this.pending = [];
  }

  get length(): number {
    return this.queue.length;
  }

  get paused(): boolean {
    return this._paused;
  }

  pause(): void {
    this._paused = true;
  }

  resume(): void {
    this._paused = false;
    for (const entry of this.pending) {
      this.queue.push(entry);
    }
    this.pending = [];
  }
}
