export const MAX_SCROLLBACK_ROWS = 5000;

export interface ScrollbackEntry<T = unknown> {
  lines: string[];
  marker?: T;
  timestamp: number;
}

export class ScrollbackBuffer<T = unknown> {
  private entries: ScrollbackEntry<T>[] = [];
  private totalLines = 0;
  private maxLines: number;

  constructor(maxLines = MAX_SCROLLBACK_ROWS) {
    this.maxLines = maxLines;
  }

  append(lines: string[], marker?: T): void {
    const entry: ScrollbackEntry<T> = {
      lines: [...lines],
      marker,
      timestamp: Date.now(),
    };
    this.entries.push(entry);
    this.totalLines += lines.length;
    this.prune();
  }

  appendLine(line: string, marker?: T): void {
    this.append([line], marker);
  }

  private prune(): void {
    while (this.totalLines > this.maxLines && this.entries.length > 0) {
      const removed = this.entries.shift();
      if (removed) {
        this.totalLines -= removed.lines.length;
      }
    }
  }

  getLines(startIndex: number, count: number): string[] {
    const result: string[] = [];
    let lineOffset = 0;
    for (const entry of this.entries) {
      const endOffset = lineOffset + entry.lines.length;
      if (endOffset > startIndex) {
        const entryStart = Math.max(0, startIndex - lineOffset);
        const entryEnd = Math.min(entry.lines.length, entryStart + (count - result.length));
        for (let i = entryStart; i < entryEnd; i++) {
          result.push(entry.lines[i]!);
        }
      }
      lineOffset = endOffset;
      if (result.length >= count) break;
    }
    return result;
  }

  getEntryContaining(lineIndex: number): ScrollbackEntry<T> | undefined {
    let offset = 0;
    for (const entry of this.entries) {
      if (lineIndex < offset + entry.lines.length) return entry;
      offset += entry.lines.length;
    }
    return undefined;
  }

  get lineCount(): number {
    return this.totalLines;
  }

  get entryCount(): number {
    return this.entries.length;
  }

  clear(): void {
    this.entries = [];
    this.totalLines = 0;
  }

  [Symbol.iterator](): Iterator<ScrollbackEntry<T>> {
    return this.entries[Symbol.iterator]();
  }
}
