export interface Renderer {
  render(lines: string[], width: number, height: number): void;
  reset(): void;
  clearScreen(): void;
  flush(): void;
  shutdown(): void;
  beginSync(): void;
  endSync(): void;
}

export const SYNCHRONIZED_OUTPUT_BEGIN = "\x1b[?2026h";
export const SYNCHRONIZED_OUTPUT_END = "\x1b[?2026l";
