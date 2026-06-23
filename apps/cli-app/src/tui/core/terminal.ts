import { setKittyProtocolActive, isKittyProtocolActive } from "./keys/index.js";
import { StdinBuffer } from "./stdin-buffer.js";

const TERMINAL_PROGRESS_KEEPALIVE_MS = 1000;
const TERMINAL_PROGRESS_ACTIVE_SEQUENCE = "\x1b]9;4;3\x07";
const TERMINAL_PROGRESS_CLEAR_SEQUENCE = "\x1b]9;4;0;\x07";

/**
 * Minimal terminal interface for TUI
 */
export interface Terminal {
  // Start the terminal with input and resize handlers
  start(onInput: (data: string) => void, onResize: () => void): void;

  // Stop the terminal and restore state
  stop(): void;

  /**
   * Drain stdin before exiting to prevent Kitty key release events from
   * leaking to the parent shell over slow SSH connections.
   */
  drainInput(maxMs?: number, idleMs?: number): Promise<void>;

  // Write output to terminal
  write(data: string): void;

  // Get terminal dimensions
  get columns(): number;
  get rows(): number;
  
  // Cursor positioning (relative to current position)
  moveBy(lines: number): void;

  // Cursor visibility
  hideCursor(): void;
  showCursor(): void;

  // Clear operations
  clearLine(): void;
  clearFromCursor(): void;
  clearScreen(): void;

  // Title operations
  setTitle(title: string): void;

  // Progress indicator (OSC 9;4)
  setProgress(active: boolean): void;
}

/**
 * Real terminal using process.stdin/stdout
 */
export class ProcessTerminal implements Terminal {
  private wasRaw = false;
  private inputHandler?: (data: string) => void;
  private resizeHandler?: () => void;
  private _modifyOtherKeysActive = false;
  private stdinBuffer?: StdinBuffer;
  private stdinDataHandler?: (data: string) => void;
  private progressInterval?: ReturnType<typeof setInterval>;


  start(onInput: (data: string) => void, onResize: () => void): void {
    this.inputHandler = onInput;
    this.resizeHandler = onResize;

    // Save previous state and enable raw mode
    this.wasRaw = process.stdin.isRaw || false;
    if (process.stdin.setRawMode) {
      process.stdin.setRawMode(true);
    }
    process.stdin.setEncoding("utf8");
    process.stdin.resume();

    // Enable bracketed paste mode
    process.stdout.write("\x1b[?2004h");

    // Set up resize handler
    process.stdout.on("resize", this.resizeHandler);

    // Refresh terminal dimensions
    if (process.platform !== "win32") {
      process.kill(process.pid, "SIGWINCH");
    }

    // Query and enable Kitty keyboard protocol
    this.queryAndEnableKittyProtocol();
  }

  /**
   * Set up StdinBuffer to split batched input into individual sequences.
   */
  private setupStdinBuffer(): void {
    this.stdinBuffer = new StdinBuffer({ timeout: 10 });

    // Kitty protocol response pattern: \x1b[?<flags>u
    const ESC = '\u001b';
    const kittyResponsePattern = new RegExp('^' + ESC + '\\[\\?(\\d+)u$');

    // Forward individual sequences to the input handler
    this.stdinBuffer.on("data", (sequence) => {
      // Check for Kitty protocol response
      if (!isKittyProtocolActive()) {
        const match = sequence.match(kittyResponsePattern);
        if (match) {
          setKittyProtocolActive(true);

          // Enable Kitty keyboard protocol (push flags)
          // Flag 1 = disambiguate escape codes
          // Flag 2 = report event types (press/repeat/release)
          // Flag 4 = report alternate keys
          process.stdout.write("\x1b[>7u");
          return; // Don't forward protocol response to TUI
        }
      }

      if (this.inputHandler) {
        this.inputHandler(sequence);
      }
    });

    // Re-wrap paste content with bracketed paste markers
    this.stdinBuffer.on("paste", (content) => {
      if (this.inputHandler) {
        this.inputHandler(`\x1b[200~${content}\x1b[201~`);
      }
    });

    // Handler that pipes stdin data through the buffer
    this.stdinDataHandler = (data: string) => {
      this.stdinBuffer!.process(data);
    };
  }

  /**
   * Query terminal for Kitty keyboard protocol support and enable if available.
   */
  private queryAndEnableKittyProtocol(): void {
    this.setupStdinBuffer();
    process.stdin.on("data", this.stdinDataHandler!);
    process.stdout.write("\x1b[?u");
    
    // Fallback to modifyOtherKeys if Kitty not detected
    setTimeout(() => {
      if (!isKittyProtocolActive() && !this._modifyOtherKeysActive) {
        process.stdout.write("\x1b[>4;2m");
        this._modifyOtherKeysActive = true;
      }
    }, 150);
  }

  async drainInput(maxMs = 1000, idleMs = 50): Promise<void> {
    if (isKittyProtocolActive()) {
      // Disable Kitty keyboard protocol
      process.stdout.write("\x1b[<u");
      setKittyProtocolActive(false);
    }
    if (this._modifyOtherKeysActive) {
      process.stdout.write("\x1b[>4;0m");
      this._modifyOtherKeysActive = false;
    }

    const previousHandler = this.inputHandler;
    this.inputHandler = undefined;

    let lastDataTime = Date.now();
    const onData = () => {
      lastDataTime = Date.now();
    };

    process.stdin.on("data", onData);
    const endTime = Date.now() + maxMs;

    try {
      while (true) {
        const now = Date.now();
        const timeLeft = endTime - now;
        if (timeLeft <= 0) break;
        if (now - lastDataTime >= idleMs) break;
        await new Promise((resolve) => setTimeout(resolve, Math.min(idleMs, timeLeft)));
      }
    } finally {
      process.stdin.removeListener("data", onData);
      this.inputHandler = previousHandler;
    }
  }

  stop(): void {
    if (this.progressInterval) {
      clearInterval(this.progressInterval);
      this.progressInterval = undefined;
      process.stdout.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
    }

    // Disable bracketed paste mode
    process.stdout.write("\x1b[?2004l");

    // Disable Kitty keyboard protocol
    if (isKittyProtocolActive()) {
      process.stdout.write("\x1b[<u");
      setKittyProtocolActive(false);
    }
    if (this._modifyOtherKeysActive) {
      process.stdout.write("\x1b[>4;0m");
      this._modifyOtherKeysActive = false;
    }

    // Clean up StdinBuffer
    if (this.stdinBuffer) {
      this.stdinBuffer.destroy();
      this.stdinBuffer = undefined;
    }

    // Remove event handlers
    if (this.stdinDataHandler) {
      process.stdin.removeListener("data", this.stdinDataHandler);
      this.stdinDataHandler = undefined;
    }
    this.inputHandler = undefined;
    if (this.resizeHandler) {
      process.stdout.removeListener("resize", this.resizeHandler);
      this.resizeHandler = undefined;
    }

    // Pause stdin
    process.stdin.pause();

    // Restore raw mode state
    if (process.stdin.setRawMode) {
      process.stdin.setRawMode(this.wasRaw);
    }
  }

  write(data: string): void {
    process.stdout.write(data);
  }

  get columns(): number {
    return process.stdout.columns || 80;
  }

  get rows(): number {
    return process.stdout.rows || 24;
  }

  moveBy(lines: number): void {
    if (lines > 0) {
      // Move down
      process.stdout.write(`\x1b[${lines}B`);
    } else if (lines < 0) {
      // Move up
      process.stdout.write(`\x1b[${-lines}A`);
    }
  }

  hideCursor(): void {
    process.stdout.write("\x1b[?25l");
  }

  showCursor(): void {
    process.stdout.write("\x1b[?25h");
  }

  clearLine(): void {
    process.stdout.write("\x1b[K");
  }

  clearFromCursor(): void {
    process.stdout.write("\x1b[J");
  }

  clearScreen(): void {
    process.stdout.write("\x1b[2J\x1b[H");
  }

  setTitle(title: string): void {
    // OSC 0;title BEL - set terminal window title
    process.stdout.write(`\x1b]0;${title}\x07`);
  }

  setProgress(active: boolean): void {
    if (active) {
      process.stdout.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
      if (!this.progressInterval) {
        this.progressInterval = setInterval(() => {
          process.stdout.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
        }, TERMINAL_PROGRESS_KEEPALIVE_MS);
      }
    } else {
      if (this.progressInterval) {
        clearInterval(this.progressInterval);
        this.progressInterval = undefined;
      }
      process.stdout.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
    }
  }
}
