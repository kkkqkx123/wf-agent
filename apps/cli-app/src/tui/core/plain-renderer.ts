import type { Renderer } from "./renderer.js";

export class PlainRenderer implements Renderer {
  constructor() {}

  render(lines: string[], _width: number, _height: number): void {
    for (const line of lines) {
      process.stdout.write(line + "\n");
    }
  }

  reset(): void {
    // No state to reset
  }

  clearScreen(): void {
    // No-op for non-TTY
  }

  flush(): void {
    // No buffering
  }

  shutdown(): void {
    // No cleanup needed
  }

  beginSync(): void {
    // No-op for non-TTY
  }

  endSync(): void {
    // No-op for non-TTY
  }
}
