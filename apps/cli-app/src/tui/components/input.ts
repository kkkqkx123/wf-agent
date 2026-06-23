import { getKeybindings } from "../core/keybindings.js";
import { decodeKittyPrintable } from "../core/keys/index.js";
import { KillRing } from "../core/kill-ring.js";
import { type Component, CURSOR_MARKER, type Focusable } from "../core/tui.js";
import { UndoStack } from "../core/undo-stack.js";
import { getSegmenter, isWhitespaceChar, visibleWidth } from "../core/utils.js";

const segmenter = getSegmenter();

interface InputState {
  value: string;
  cursor: number;
}

/**
 * Input component - single-line text input with horizontal scrolling
 */
export class Input implements Component, Focusable {
  private value: string = "";
  private cursor: number = 0; // Cursor position in the value
  public onSubmit?: (value: string) => void;
  public onEscape?: () => void;
  public placeholder?: string;

  /** Focusable interface - set by TUI when focus changes */
  focused: boolean = false;

  // Bracketed paste mode buffering
  private pasteBuffer: string = "";
  private isInPaste: boolean = false;

  // Kill ring for Emacs-style kill/yank operations
  private killRing = new KillRing();
  private lastAction: "kill" | "yank" | "type-word" | null = null;

  // Undo support
  private undoStack = new UndoStack<InputState>();

  constructor(placeholder?: string) {
    this.placeholder = placeholder;
  }

  getValue(): string {
    return this.value;
  }

  setValue(value: string): void {
    this.value = value;
    this.cursor = Math.min(this.cursor, value.length);
  }

  handleInput(data: string): void {
    // Handle bracketed paste mode
    if (data.includes("\x1b[200~")) {
      this.isInPaste = true;
      this.pasteBuffer = "";
      data = data.replace("\x1b[200~", "");
    }

    // If we're in a paste, buffer the data
    if (this.isInPaste) {
      this.pasteBuffer += data;

      const endIndex = this.pasteBuffer.indexOf("\x1b[201~");
      if (endIndex !== -1) {
        const pasteContent = this.pasteBuffer.substring(0, endIndex);
        this.handlePaste(pasteContent);
        this.isInPaste = false;

        const remaining = this.pasteBuffer.substring(endIndex + 6);
        this.pasteBuffer = "";
        if (remaining) {
          this.handleInput(remaining);
        }
      }
      return;
    }

    const kb = getKeybindings();

    // Escape/Cancel
    if (kb.matches(data, "tui.select.cancel")) {
      if (this.onEscape) this.onEscape();
      return;
    }

    // Undo
    if (kb.matches(data, "tui.editor.undo")) {
      this.undo();
      return;
    }

    // Submit
    if (kb.matches(data, "tui.input.submit") || data === "\n") {
      if (this.onSubmit) this.onSubmit(this.value);
      return;
    }

    // Deletion
    if (kb.matches(data, "tui.editor.deleteCharBackward")) {
      this.handleBackspace();
      return;
    }

    if (kb.matches(data, "tui.editor.deleteCharForward")) {
      this.handleForwardDelete();
      return;
    }

    if (kb.matches(data, "tui.editor.deleteWordBackward")) {
      this.deleteWordBackwards();
      return;
    }

    if (kb.matches(data, "tui.editor.deleteToLineStart")) {
      this.deleteToLineStart();
      return;
    }

    if (kb.matches(data, "tui.editor.deleteToLineEnd")) {
      this.deleteToLineEnd();
      return;
    }

    // Kill ring actions
    if (kb.matches(data, "tui.editor.yank")) {
      this.yank();
      return;
    }

    // Cursor movement
    if (kb.matches(data, "tui.editor.cursorLeft")) {
      this.lastAction = null;
      if (this.cursor > 0) {
        const beforeCursor = this.value.slice(0, this.cursor);
        const graphemes = [...segmenter.segment(beforeCursor)];
        const lastGrapheme = graphemes[graphemes.length - 1];
        this.cursor -= lastGrapheme ? lastGrapheme.segment.length : 1;
      }
      return;
    }

    if (kb.matches(data, "tui.editor.cursorRight")) {
      this.lastAction = null;
      if (this.cursor < this.value.length) {
        const afterCursor = this.value.slice(this.cursor);
        const graphemes = [...segmenter.segment(afterCursor)];
        const firstGrapheme = graphemes[0];
        this.cursor += firstGrapheme ? firstGrapheme.segment.length : 1;
      }
      return;
    }

    if (kb.matches(data, "tui.editor.cursorLineStart")) {
      this.lastAction = null;
      this.cursor = 0;
      return;
    }

    if (kb.matches(data, "tui.editor.cursorLineEnd")) {
      this.lastAction = null;
      this.cursor = this.value.length;
      return;
    }

    if (kb.matches(data, "tui.editor.cursorWordLeft")) {
      this.moveWordBackwards();
      return;
    }

    if (kb.matches(data, "tui.editor.cursorWordRight")) {
      this.moveWordForwards();
      return;
    }

    // Kitty CSI-u printable character
    const kittyPrintable = decodeKittyPrintable(data);
    if (kittyPrintable !== undefined && kittyPrintable !== null) {
      this.insertCharacter(kittyPrintable);
      return;
    }

    // Regular character input
    const hasControlChars = [...data].some((ch) => {
      const code = ch.charCodeAt(0);
      return code < 32 || code === 0x7f || (code >= 0x80 && code <= 0x9f);
    });
    if (!hasControlChars) {
      this.insertCharacter(data);
    }
  }

  private handlePaste(content: string): void {
    this.pushUndo();
    this.value = this.value.slice(0, this.cursor) + content + this.value.slice(this.cursor);
    this.cursor += content.length;
  }

  private insertCharacter(char: string): void {
    // Undo coalescing: consecutive word chars coalesce into one undo unit
    if (isWhitespaceChar(char) || this.lastAction !== "type-word") {
      this.pushUndo();
    }
    this.lastAction = "type-word";

    this.value = this.value.slice(0, this.cursor) + char + this.value.slice(this.cursor);
    this.cursor += char.length;
  }

  private handleBackspace(): void {
    this.lastAction = null;
    if (this.cursor > 0) {
      this.pushUndo();
      const beforeCursor = this.value.slice(0, this.cursor);
      const graphemes = [...segmenter.segment(beforeCursor)];
      const lastGrapheme = graphemes[graphemes.length - 1];
      const graphemeLength = lastGrapheme ? lastGrapheme.segment.length : 1;
      this.value =
        this.value.slice(0, this.cursor - graphemeLength) + this.value.slice(this.cursor);
      this.cursor -= graphemeLength;
    }
  }

  private handleForwardDelete(): void {
    this.lastAction = null;
    if (this.cursor < this.value.length) {
      this.pushUndo();
      const afterCursor = this.value.slice(this.cursor);
      const graphemes = [...segmenter.segment(afterCursor)];
      const firstGrapheme = graphemes[0];
      const graphemeLength = firstGrapheme ? firstGrapheme.segment.length : 1;
      this.value =
        this.value.slice(0, this.cursor) + this.value.slice(this.cursor + graphemeLength);
    }
  }

  private deleteToLineStart(): void {
    if (this.cursor === 0) return;
    this.pushUndo();
    const killed = this.value.slice(0, this.cursor);
    this.killRing.kill(killed);
    this.value = this.value.slice(this.cursor);
    this.cursor = 0;
    this.lastAction = "kill";
  }

  private deleteToLineEnd(): void {
    if (this.cursor >= this.value.length) return;
    this.pushUndo();
    const killed = this.value.slice(this.cursor);
    this.killRing.kill(killed);
    this.value = this.value.slice(0, this.cursor);
    this.lastAction = "kill";
  }

  private deleteWordBackwards(): void {
    if (this.cursor === 0) return;
    this.pushUndo();
    let pos = this.cursor;
    const before = this.value.slice(0, pos);
    const graphemes = [...segmenter.segment(before)].reverse();

    // Skip whitespace
    while (graphemes.length > 0 && isWhitespaceChar(graphemes[0]!.segment)) {
      pos -= graphemes.shift()!.segment.length;
    }

    // Skip word characters
    while (graphemes.length > 0 && !isWhitespaceChar(graphemes[0]!.segment)) {
      pos -= graphemes.shift()!.segment.length;
    }

    const killed = this.value.slice(pos, this.cursor);
    this.killRing.kill(killed);
    this.value = this.value.slice(0, pos) + this.value.slice(this.cursor);
    this.cursor = pos;
    this.lastAction = "kill";
  }

  private moveWordBackwards(): void {
    if (this.cursor === 0) return;
    this.lastAction = null;
    let pos = this.cursor;
    const before = this.value.slice(0, pos);
    const graphemes = [...segmenter.segment(before)].reverse();

    // Skip whitespace
    while (graphemes.length > 0 && isWhitespaceChar(graphemes[0]!.segment)) {
      pos -= graphemes.shift()!.segment.length;
    }

    // Skip word characters
    while (graphemes.length > 0 && !isWhitespaceChar(graphemes[0]!.segment)) {
      pos -= graphemes.shift()!.segment.length;
    }

    this.cursor = pos;
  }

  private moveWordForwards(): void {
    if (this.cursor >= this.value.length) return;
    this.lastAction = null;
    let pos = this.cursor;
    const after = this.value.slice(pos);
    const graphemes = [...segmenter.segment(after)];

    // Skip word characters
    while (graphemes.length > 0 && !isWhitespaceChar(graphemes[0]!.segment)) {
      pos += graphemes.shift()!.segment.length;
    }

    // Skip whitespace
    while (graphemes.length > 0 && isWhitespaceChar(graphemes[0]!.segment)) {
      pos += graphemes.shift()!.segment.length;
    }

    this.cursor = pos;
  }

  private yank(): void {
    const text = this.killRing.yank();
    if (text) {
      this.pushUndo();
      this.value = this.value.slice(0, this.cursor) + text + this.value.slice(this.cursor);
      this.cursor += text.length;
      this.lastAction = "yank";
    }
  }

  private pushUndo(): void {
    this.undoStack.push({ value: this.value, cursor: this.cursor });
  }

  private undo(): void {
    const state = this.undoStack.undo();
    if (state) {
      this.value = state.value;
      this.cursor = state.cursor;
    }
  }

  invalidate(): void {
    // No cached state to invalidate currently
  }

  render(width: number): string[] {
    // Build display text with cursor marker
    let displayText = this.value;
    if (displayText === "" && this.placeholder) {
      displayText = this.placeholder;
    }

    // Insert cursor marker
    const cursorPos = displayText === this.placeholder ? 0 : this.cursor;
    const beforeCursor = displayText.slice(0, cursorPos);
    const afterCursor = displayText.slice(cursorPos);
    const withCursor = beforeCursor + CURSOR_MARKER + afterCursor;

    // Calculate visible width and handle horizontal scrolling
    const totalWidth = visibleWidth(withCursor);
    let startCol = 0;

    if (totalWidth > width) {
      // Need to scroll - keep cursor visible
      const beforeCursorWidth = visibleWidth(beforeCursor);
      if (beforeCursorWidth >= width - 5) {
        // Scroll so cursor is ~5 chars from right edge
        startCol = beforeCursorWidth - (width - 5);
      }
    }

    // Extract visible portion
    const { text: visibleText } = extractSegment(withCursor, startCol, width);

    return [visibleText];
  }
}

/**
 * Extract a segment of text starting from a column position
 */
function extractSegment(text: string, startCol: number, maxWidth: number): {
  text: string;
  width: number;
} {
  let currentWidth = 0;
  let extractedWidth = 0;
  let result = "";
  let i = 0;

  // Move to start position
  while (i < text.length && currentWidth < startCol) {
    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    currentWidth += visibleWidth(segment);
    i += segment.length;
  }

  // Extract up to maxWidth
  while (i < text.length && extractedWidth < maxWidth) {
    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = visibleWidth(segment);

    if (extractedWidth + segWidth > maxWidth) {
      break;
    }

    result += segment;
    extractedWidth += segWidth;
    i += segment.length;
  }

  return { text: result, width: extractedWidth };
}
