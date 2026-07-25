export enum Priority {
  Critical = 0,
  Input = 1,
  Render = 2,
  Animation = 3,
  Poll = 4,
  Idle = 5,
}

export interface TickSource {
  interval: number;
  handler: () => void;
  priority: Priority;
  enabled: () => boolean;
}

interface ScheduledTask {
  id: string;
  handler: () => void;
  priority: Priority;
}

export class EventScheduler {
  private sources: Map<string, { source: TickSource; timer: ReturnType<typeof setInterval> | undefined; lastTick: number }> = new Map();
  private running = false;
  private deferredTasks: ScheduledTask[] = [];
  private processingDeferred = false;

  addSource(name: string, source: TickSource): void {
    if (this.sources.has(name)) {
      this.removeSource(name);
    }
    this.sources.set(name, { source, timer: undefined, lastTick: 0 });
    if (this.running) {
      this.startSource(name);
    }
  }

  removeSource(name: string): void {
    const entry = this.sources.get(name);
    if (entry?.timer !== undefined) {
      clearInterval(entry.timer);
    }
    this.sources.delete(name);
  }

  deferTask(id: string, handler: () => void, priority: Priority = Priority.Idle): void {
    this.deferredTasks.push({ id, handler, priority });
    if (!this.processingDeferred) {
      this.processingDeferred = true;
      queueMicrotask(() => this.processDeferredTasks());
    }
  }

  cancelDeferred(id: string): void {
    this.deferredTasks = this.deferredTasks.filter((t) => t.id !== id);
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    for (const name of this.sources.keys()) {
      this.startSource(name);
    }
  }

  stop(): void {
    this.running = false;
    for (const [, entry] of this.sources) {
      if (entry.timer !== undefined) {
        clearInterval(entry.timer);
        entry.timer = undefined;
      }
    }
    this.deferredTasks = [];
    this.processingDeferred = false;
  }

  hasSource(name: string): boolean {
    return this.sources.has(name);
  }

  getTickCount(name: string): number {
    return this.sources.get(name)?.lastTick ?? 0;
  }

  private startSource(name: string): void {
    const entry = this.sources.get(name);
    if (!entry || !this.running) return;

    const tick = (): void => {
      if (!entry.source.enabled()) return;
      entry.lastTick++;
      entry.source.handler();
    };

    entry.timer = setInterval(tick, entry.source.interval);
  }

  private processDeferredTasks(): void {
    this.deferredTasks.sort((a, b) => a.priority - b.priority);
    const tasks = this.deferredTasks;
    this.deferredTasks = [];
    this.processingDeferred = false;

    for (const task of tasks) {
      task.handler();
    }
  }
}
