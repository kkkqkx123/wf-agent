import type { AutocompleteProvider, AutocompleteSuggestions } from "../core/autocomplete.js";
import { getKeybindings } from "../core/keybindings.js";
import { decodePrintableKey, matchesKey } from "../core/keys/index.js";
import { KillRing } from "../core/kill-ring.js";
import { type Component, CURSOR_MARKER, type Focusable, type TUI } from "../core/tui.js";
import { UndoStack } from "../core/undo-stack.js";
import { getSegmenter, isPunctuationChar, isWhitespaceChar, truncateToWidth, visibleWidth, wordWrapLine } from "../core/utils.js";
import { SelectList, type SelectListLayoutOptions, type SelectListTheme } from "./select-list.js";

const baseSegmenter = getSegmenter();

interface EditorState {
  lines: string[];
  cursorLine: number;
  cursorCol: number;
}

interface LayoutLine {
  text: string;
  hasCursor: boolean;
  cursorPos?: number;
}

export interface EditorTheme {
  borderColor: (str: string) => string;
  selectList: SelectListTheme;
}

export interface EditorOptions {
  paddingX?: number;
  autocompleteMaxVisible?: number;
}

const SLASH_COMMAND_SELECT_LIST_LAYOUT: SelectListLayoutOptions = {
  minPrimaryColumnWidth: 12,
  maxPrimaryColumnWidth: 32,
};

export class Editor implements Component, Focusable {
  private state: EditorState = {
    lines: [""],
    cursorLine: 0,
    cursorCol: 0,
  };

  /** Focusable interface - set by TUI when focus changes */
  focused: boolean = false;

  protected tui: TUI;
  private theme: EditorTheme;
  private paddingX: number = 0;

  // Store last render width for cursor navigation
  private lastWidth: number = 80;

  // Vertical scrolling support
  private scrollOffset: number = 0;

  // Border color (can be changed dynamically)
  public borderColor: (str: string) => string;

  // Autocomplete support
  private autocompleteProvider?: AutocompleteProvider;
  private autocompleteList?: SelectList;
  private autocompleteState: "regular" | "force" | null = null;
  private autocompleteIsSlashCommand: boolean = false;
  private autocompleteMaxVisible: number = 5;
  private autocompleteAbort?: AbortController;
  private autocompleteDebounceTimer?: ReturnType<typeof setTimeout>;
  private autocompleteRequestTask: Promise<void> = Promise.resolve();
  private autocompleteStartToken: number = 0;
  private autocompleteRequestId: number = 0;

  // Prompt history for up/down navigation
  private history: string[] = [];
  private historyIndex: number = -1; // -1 = not browsing, 0 = most recent, 1 = older, etc.

  // Kill ring for Emacs-style kill/yank operations
  private killRing = new KillRing();
  private lastAction: "kill" | "yank" | "type-word" | null = null;

  // Preferred visual column for vertical cursor movement (sticky column)
  private preferredVisualCol: number | null = null;

  // Undo support
  private undoStack = new UndoStack<EditorState>();

  public onSubmit?: (text: string) => void;
  public onChange?: (text: string) => void;
  /** Called when Esc is pressed and editor is empty — used to switch to Normal mode */
  public onModeSwitch?: () => void;
  public disableSubmit: boolean = false;

  constructor(tui: TUI, theme: EditorTheme, options: EditorOptions = {}) {
    this.tui = tui;
    this.theme = theme;
    this.borderColor = theme.borderColor;
    const paddingX = options.paddingX ?? 0;
    this.paddingX = Number.isFinite(paddingX) ? Math.max(0, Math.floor(paddingX)) : 0;
    const maxVisible = options.autocompleteMaxVisible ?? 5;
    this.autocompleteMaxVisible = Number.isFinite(maxVisible) ? Math.max(3, Math.min(20, Math.floor(maxVisible))) : 5;
  }

  getPaddingX(): number {
    return this.paddingX;
  }

  setPaddingX(padding: number): void {
    const newPadding = Number.isFinite(padding) ? Math.max(0, Math.floor(padding)) : 0;
    if (this.paddingX !== newPadding) {
      this.paddingX = newPadding;
      this.tui.requestRender();
    }
  }

  getAutocompleteMaxVisible(): number {
    return this.autocompleteMaxVisible;
  }

  setAutocompleteMaxVisible(maxVisible: number): void {
    const newMaxVisible = Number.isFinite(maxVisible) ? Math.max(3, Math.min(20, Math.floor(maxVisible))) : 5;
    if (this.autocompleteMaxVisible !== newMaxVisible) {
      this.autocompleteMaxVisible = newMaxVisible;
      this.tui.requestRender();
    }
  }

  setAutocompleteProvider(provider: AutocompleteProvider): void {
    this.cancelAutocomplete();
    this.autocompleteProvider = provider;
  }

  /**
   * Add a prompt to history for up/down arrow navigation.
   * Called after successful submission.
   */
  addToHistory(text: string): void {
    const trimmed = text.trim();
    if (!trimmed) return;
    // Don't add consecutive duplicates
    if (this.history.length > 0 && this.history[0] === trimmed) return;
    this.history.unshift(trimmed);
    // Limit history size
    if (this.history.length > 100) {
      this.history.pop();
    }
  }

  private isEditorEmpty(): boolean {
    return this.state.lines.length === 1 && this.state.lines[0] === "";
  }

  private isOnFirstVisualLine(): boolean {
    const visualLines = this.buildVisualLineMap(this.lastWidth);
    const currentVisualLine = this.findCurrentVisualLine(visualLines);
    return currentVisualLine === 0;
  }

  private isOnLastVisualLine(): boolean {
    const visualLines = this.buildVisualLineMap(this.lastWidth);
    const currentVisualLine = this.findCurrentVisualLine(visualLines);
    return currentVisualLine === visualLines.length - 1;
  }

  private navigateHistory(direction: 1 | -1): void {
    this.lastAction = null;
    if (this.history.length === 0) return;

    const newIndex = this.historyIndex - direction; // Up(-1) increases index, Down(1) decreases
    if (newIndex < -1 || newIndex >= this.history.length) return;

    // Capture state when first entering history browsing mode
    if (this.historyIndex === -1 && newIndex >= 0) {
      this.pushUndoSnapshot();
    }

    this.historyIndex = newIndex;

    if (this.historyIndex === -1) {
      // Returned to "current" state - clear editor
      this.setTextInternal("");
    } else {
      this.setTextInternal(this.history[this.historyIndex] || "");
    }
  }

  /** Internal setText that doesn't reset history state - used by navigateHistory */
  private setTextInternal(text: string): void {
    const lines = text.split("\n");
    this.state.lines = lines.length === 0 ? [""] : lines;
    this.state.cursorLine = this.state.lines.length - 1;
    this.setCursorCol(this.state.lines[this.state.cursorLine]?.length || 0);
    // Reset scroll - render() will adjust to show cursor
    this.scrollOffset = 0;

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  invalidate(): void {
    // No cached state to invalidate currently
  }

  render(width: number): string[] {
    const maxPadding = Math.max(0, Math.floor((width - 1) / 2));
    const paddingX = Math.min(this.paddingX, maxPadding);
    const contentWidth = Math.max(1, width - paddingX * 2);

    // Layout width: with padding the cursor can overflow into it,
    // without padding we reserve 1 column for the cursor.
    const layoutWidth = Math.max(1, contentWidth - (paddingX ? 0 : 1));

    // Store for cursor navigation (must match wrapping width)
    this.lastWidth = layoutWidth;

    const horizontal = this.borderColor("─");

    // Layout the text
    const layoutLines = this.layoutText(layoutWidth);

    // Calculate max visible lines: 30% of terminal height, minimum 5 lines
    const terminalRows = this.tui.terminal.rows;
    const maxVisibleLines = Math.max(5, Math.floor(terminalRows * 0.3));

    // Find the cursor line index in layoutLines
    let cursorLineIndex = layoutLines.findIndex((line) => line.hasCursor);
    if (cursorLineIndex === -1) cursorLineIndex = 0;

    // Adjust scroll offset to keep cursor visible
    if (cursorLineIndex < this.scrollOffset) {
      this.scrollOffset = cursorLineIndex;
    } else if (cursorLineIndex >= this.scrollOffset + maxVisibleLines) {
      this.scrollOffset = cursorLineIndex - maxVisibleLines + 1;
    }

    // Clamp scroll offset to valid range
    const maxScrollOffset = Math.max(0, layoutLines.length - maxVisibleLines);
    this.scrollOffset = Math.max(0, Math.min(this.scrollOffset, maxScrollOffset));

    // Get visible lines slice
    const visibleLines = layoutLines.slice(this.scrollOffset, this.scrollOffset + maxVisibleLines);

    const result: string[] = [];
    const leftPadding = " ".repeat(paddingX);
    const rightPadding = leftPadding;

    // Render top border (with scroll indicator if scrolled down)
    if (this.scrollOffset > 0) {
      const indicator = `─── ↑ ${this.scrollOffset} more `;
      const remaining = width - visibleWidth(indicator);
      if (remaining >= 0) {
        result.push(this.borderColor(indicator + "─".repeat(remaining)));
      } else {
        result.push(this.borderColor(truncateToWidth(indicator, width)));
      }
    } else {
      result.push(horizontal.repeat(width));
    }

    // Render each visible layout line
    // Emit hardware cursor marker only when focused and not showing autocomplete
    const emitCursorMarker = this.focused && !this.autocompleteState;

    for (const layoutLine of visibleLines) {
      let displayText = layoutLine.text;
      let lineVisibleWidth = visibleWidth(layoutLine.text);
      let cursorInPadding = false;

      // Add cursor if this line has it
      if (layoutLine.hasCursor && layoutLine.cursorPos !== undefined) {
        const before = displayText.slice(0, layoutLine.cursorPos);
        const after = displayText.slice(layoutLine.cursorPos);

        // Hardware cursor marker (zero-width, emitted before fake cursor for IME positioning)
        const marker = emitCursorMarker ? CURSOR_MARKER : "";

        if (after.length > 0) {
          // Cursor is on a character (grapheme) - replace it with highlighted version
          // Get the first grapheme from 'after'
          const afterGraphemes = [...baseSegmenter.segment(after)];
          const firstGrapheme = afterGraphemes[0]?.segment || "";
          const restAfter = after.slice(firstGrapheme.length);
          const cursor = `\x1b[7m${firstGrapheme}\x1b[0m`;
          displayText = before + marker + cursor + restAfter;
          // lineVisibleWidth stays the same - we're replacing, not adding
        } else {
          // Cursor is at the end - add highlighted space
          const cursor = "\x1b[7m \x1b[0m";
          displayText = before + marker + cursor;
          lineVisibleWidth = lineVisibleWidth + 1;
          // If cursor overflows content width into the padding, flag it
          if (lineVisibleWidth > contentWidth && paddingX > 0) {
            cursorInPadding = true;
          }
        }
      }

      // Calculate padding based on actual visible width
      const padding = " ".repeat(Math.max(0, contentWidth - lineVisibleWidth));
      const lineRightPadding = cursorInPadding ? rightPadding.slice(1) : rightPadding;

      // Render the line (no side borders, just horizontal lines above and below)
      result.push(`${leftPadding}${displayText}${padding}${lineRightPadding}`);
    }

    // Render bottom border (with scroll indicator if more content below)
    const linesBelow = layoutLines.length - (this.scrollOffset + visibleLines.length);
    if (linesBelow > 0) {
      const indicator = `─── ↓ ${linesBelow} more `;
      const remaining = width - visibleWidth(indicator);
      result.push(this.borderColor(indicator + "─".repeat(Math.max(0, remaining))));
    } else {
      result.push(horizontal.repeat(width));
    }

    // Add autocomplete list if active
    if (this.autocompleteState && this.autocompleteList) {
      const autocompleteResult = this.autocompleteList.render(contentWidth);
      for (const line of autocompleteResult) {
        const lineWidth = visibleWidth(line);
        const linePadding = " ".repeat(Math.max(0, contentWidth - lineWidth));
        result.push(`${leftPadding}${line}${linePadding}${rightPadding}`);
      }
    }

    return result;
  }

  handleInput(data: string): void {
    const kb = getKeybindings();

    // Handle bracketed paste mode
    if (data.includes("\x1b[200~")) {
      // Simplified paste handling - just insert the content
      const ESC = '\u001b';
      const pasteContent = data.replace(new RegExp(ESC + '\\[200~|' + ESC + '\\[201~', 'g'), "");
      if (pasteContent.length > 0) {
        this.insertTextAtCursor(pasteContent);
      }
      return;
    }

    // Ctrl+C - let parent handle (exit/clear)
    if (kb.matches(data, "tui.input.copy")) {
      return;
    }

    // Undo
    if (kb.matches(data, "tui.editor.undo")) {
      this.undo();
      return;
    }

    // Handle autocomplete mode
    if (this.autocompleteState && this.autocompleteList) {
      if (kb.matches(data, "tui.select.cancel")) {
        this.cancelAutocomplete();
        return;
      }

      if (kb.matches(data, "tui.select.up") || kb.matches(data, "tui.select.down")) {
        this.autocompleteList.handleInput(data);
        return;
      }

      if (kb.matches(data, "tui.input.tab")) {
        const selected = this.autocompleteList.getSelectedItem();
        if (selected) {
          // Insert selected value at cursor
          this.pushUndoSnapshot();
          this.lastAction = null;
          const value = selected.value || selected.label;
          this.insertTextAtCursor(value);
          this.cancelAutocomplete();
          if (this.onChange) this.onChange(this.getText());
        }
        return;
      }

      if (kb.matches(data, "tui.select.confirm")) {
        const selected = this.autocompleteList.getSelectedItem();
        if (selected) {
          // Insert selected value at cursor
          this.pushUndoSnapshot();
          this.lastAction = null;
          const value = selected.value || selected.label;
          this.insertTextAtCursor(value);
          this.cancelAutocomplete();
          if (this.onChange) this.onChange(this.getText());
          return;
        }
      }
    }

    // Tab - trigger completion
    if (kb.matches(data, "tui.input.tab") && !this.autocompleteState) {
      this.handleTabCompletion();
      return;
    }

    // Esc (when not in autocomplete mode) — notify parent for mode switch
    if (kb.matches(data, "tui.select.cancel") && this.onModeSwitch) {
      this.onModeSwitch();
      return;
    }

    // Deletion actions
    if (kb.matches(data, "tui.editor.deleteToLineEnd")) {
      this.deleteToEndOfLine();
      return;
    }
    if (kb.matches(data, "tui.editor.deleteToLineStart")) {
      this.deleteToStartOfLine();
      return;
    }
    if (kb.matches(data, "tui.editor.deleteWordBackward")) {
      this.deleteWordBackwards();
      return;
    }
    if (kb.matches(data, "tui.editor.deleteWordForward")) {
      this.deleteWordForward();
      return;
    }
    if (kb.matches(data, "tui.editor.deleteCharBackward") || matchesKey(data, "shift+backspace")) {
      this.handleBackspace();
      return;
    }
    if (kb.matches(data, "tui.editor.deleteCharForward") || matchesKey(data, "shift+delete")) {
      this.handleForwardDelete();
      return;
    }

    // Kill ring actions
    if (kb.matches(data, "tui.editor.yank")) {
      this.yank();
      return;
    }
    if (kb.matches(data, "tui.editor.yankPop")) {
      this.yankPop();
      return;
    }

    // Cursor movement actions
    if (kb.matches(data, "tui.editor.cursorLineStart")) {
      this.moveToLineStart();
      return;
    }
    if (kb.matches(data, "tui.editor.cursorLineEnd")) {
      this.moveToLineEnd();
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

    // New line
    if (
      kb.matches(data, "tui.input.newLine") ||
      (data.charCodeAt(0) === 10 && data.length > 1) ||
      data === "\x1b\r" ||
      data === "\x1b[13;2~" ||
      (data.length > 1 && data.includes("\x1b") && data.includes("\r")) ||
      (data === "\n" && data.length === 1)
    ) {
      this.addNewLine();
      return;
    }

    // Submit (Enter)
    if (kb.matches(data, "tui.input.submit")) {
      if (this.disableSubmit) return;
      this.submitValue();
      return;
    }

    // Arrow key navigation (with history support)
    if (kb.matches(data, "tui.editor.cursorUp")) {
      if (this.isEditorEmpty()) {
        this.navigateHistory(-1);
      } else if (this.historyIndex > -1 && this.isOnFirstVisualLine()) {
        this.navigateHistory(-1);
      } else if (this.isOnFirstVisualLine()) {
        // Already at top - jump to start of line
        this.moveToLineStart();
      } else {
        this.moveCursor(-1, 0);
      }
      return;
    }
    if (kb.matches(data, "tui.editor.cursorDown")) {
      if (this.historyIndex > -1 && this.isOnLastVisualLine()) {
        this.navigateHistory(1);
      } else if (this.isOnLastVisualLine()) {
        // Already at bottom - jump to end of line
        this.moveToLineEnd();
      } else {
        this.moveCursor(1, 0);
      }
      return;
    }
    if (kb.matches(data, "tui.editor.cursorRight")) {
      this.moveCursor(0, 1);
      return;
    }
    if (kb.matches(data, "tui.editor.cursorLeft")) {
      this.moveCursor(0, -1);
      return;
    }

    // Page up/down - scroll by page and move cursor
    if (kb.matches(data, "tui.editor.pageUp")) {
      this.pageScroll(-1);
      return;
    }
    if (kb.matches(data, "tui.editor.pageDown")) {
      this.pageScroll(1);
      return;
    }

    // Shift+Space - insert regular space
    if (matchesKey(data, "shift+space")) {
      this.insertCharacter(" ");
      return;
    }

    const printable = decodePrintableKey(data);
    if (printable !== undefined && printable !== null) {
      this.insertCharacter(printable);
      return;
    }

    // Regular characters
    if (data.charCodeAt(0) >= 32) {
      this.insertCharacter(data);
    }
  }

  private layoutText(contentWidth: number): LayoutLine[] {
    const layoutLines: LayoutLine[] = [];

    if (this.state.lines.length === 0 || (this.state.lines.length === 1 && this.state.lines[0] === "")) {
      // Empty editor
      layoutLines.push({
        text: "",
        hasCursor: true,
        cursorPos: 0,
      });
      return layoutLines;
    }

    // Process each logical line
    for (let i = 0; i < this.state.lines.length; i++) {
      const line = this.state.lines[i] || "";
      const isCurrentLine = i === this.state.cursorLine;
      const lineVisibleWidth = visibleWidth(line);

      if (lineVisibleWidth <= contentWidth) {
        // Line fits in one layout line
        if (isCurrentLine) {
          layoutLines.push({
            text: line,
            hasCursor: true,
            cursorPos: this.state.cursorCol,
          });
        } else {
          layoutLines.push({
            text: line,
            hasCursor: false,
          });
        }
      } else {
        // Line needs wrapping - use word-aware wrapping
        const chunks = wordWrapLine(line, contentWidth);

        for (let chunkIndex = 0; chunkIndex < chunks.length; chunkIndex++) {
          const chunk = chunks[chunkIndex];
          if (!chunk) continue;

          const cursorPos = this.state.cursorCol;
          const isLastChunk = chunkIndex === chunks.length - 1;

          // Determine if cursor is in this chunk
          let hasCursorInChunk = false;
          let adjustedCursorPos = 0;

          if (isCurrentLine) {
            if (isLastChunk) {
              // Last chunk: cursor belongs here if >= startIndex
              hasCursorInChunk = cursorPos >= chunk.startIndex;
              adjustedCursorPos = cursorPos - chunk.startIndex;
            } else {
              // Non-last chunk: cursor belongs here if in range [startIndex, endIndex)
              hasCursorInChunk = cursorPos >= chunk.startIndex && cursorPos < chunk.endIndex;
              if (hasCursorInChunk) {
                adjustedCursorPos = cursorPos - chunk.startIndex;
                // Clamp to text length (in case cursor was in trimmed whitespace)
                if (adjustedCursorPos > chunk.text.length) {
                  adjustedCursorPos = chunk.text.length;
                }
              }
            }
          }

          if (hasCursorInChunk) {
            layoutLines.push({
              text: chunk.text,
              hasCursor: true,
              cursorPos: adjustedCursorPos,
            });
          } else {
            layoutLines.push({
              text: chunk.text,
              hasCursor: false,
            });
          }
        }
      }
    }

    return layoutLines;
  }

  getText(): string {
    return this.state.lines.join("\n");
  }

  getLines(): string[] {
    return [...this.state.lines];
  }

  getCursor(): { line: number; col: number } {
    return { line: this.state.cursorLine, col: this.state.cursorCol };
  }

  setText(text: string): void {
    this.cancelAutocomplete();
    this.lastAction = null;
    this.historyIndex = -1; // Exit history browsing mode
    const normalized = this.normalizeText(text);
    // Push undo snapshot if content differs (makes programmatic changes undoable)
    if (this.getText() !== normalized) {
      this.pushUndoSnapshot();
    }
    this.setTextInternal(normalized);
  }

  /**
   * Insert text at the current cursor position.
   * Used for programmatic insertion (e.g., clipboard image markers).
   * This is atomic for undo - single undo restores entire pre-insert state.
   */
  insertTextAtCursor(text: string): void {
    if (!text) return;
    this.cancelAutocomplete();
    this.pushUndoSnapshot();
    this.lastAction = null;
    this.historyIndex = -1;
    this.insertTextAtCursorInternal(text);
  }

  /**
   * Normalize text for editor storage:
   * - Normalize line endings (\r\n and \r -> \n)
   * - Expand tabs to 4 spaces
   */
  private normalizeText(text: string): string {
    return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").replace(/\t/g, "    ");
  }

  /**
   * Internal text insertion at cursor. Handles single and multi-line text.
   * Does not push undo snapshots or trigger autocomplete - caller is responsible.
   * Normalizes line endings and calls onChange once at the end.
   */
  private insertTextAtCursorInternal(text: string): void {
    if (!text) return;

    // Normalize line endings and tabs
    const normalized = this.normalizeText(text);
    const insertedLines = normalized.split("\n");

    const currentLine = this.state.lines[this.state.cursorLine] || "";
    const beforeCursor = currentLine.slice(0, this.state.cursorCol);
    const afterCursor = currentLine.slice(this.state.cursorCol);

    if (insertedLines.length === 1) {
      // Single line - insert at cursor position
      this.state.lines[this.state.cursorLine] = beforeCursor + normalized + afterCursor;
      this.setCursorCol(this.state.cursorCol + normalized.length);
    } else {
      // Multi-line insertion
      this.state.lines = [
        // All lines before current line
        ...this.state.lines.slice(0, this.state.cursorLine),

        // The first inserted line merged with text before cursor
        beforeCursor + insertedLines[0],

        // All middle inserted lines
        ...insertedLines.slice(1, -1),

        // The last inserted line with text after cursor
        insertedLines[insertedLines.length - 1] + afterCursor,

        // All lines after current line
        ...this.state.lines.slice(this.state.cursorLine + 1),
      ];

      this.state.cursorLine += insertedLines.length - 1;
      this.setCursorCol((insertedLines[insertedLines.length - 1] || "").length);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private insertCharacter(char: string): void {
    this.historyIndex = -1; // Exit history browsing mode

    // Undo coalescing: consecutive word chars coalesce into one undo unit
    if (isWhitespaceChar(char) || this.lastAction !== "type-word") {
      this.pushUndoSnapshot();
    }
    this.lastAction = "type-word";

    const line = this.state.lines[this.state.cursorLine] || "";

    const before = line.slice(0, this.state.cursorCol);
    const after = line.slice(this.state.cursorCol);

    this.state.lines[this.state.cursorLine] = before + char + after;
    this.setCursorCol(this.state.cursorCol + char.length);

    if (this.onChange) {
      this.onChange(this.getText());
    }

    // Check if we should trigger autocomplete
    if (!this.autocompleteState) {
      // Auto-trigger for "/" at the start of a line (slash commands)
      if (char === "/" && this.isAtStartOfMessage()) {
        this.tryTriggerAutocomplete();
      }
      // Auto-trigger for symbol-based completion like @ or # at token boundaries
      else if (char === "@" || char === "#") {
        const currentLine = this.state.lines[this.state.cursorLine] || "";
        const textBeforeCursor = currentLine.slice(0, this.state.cursorCol);
        const charBeforeSymbol = textBeforeCursor[textBeforeCursor.length - 2];
        if (textBeforeCursor.length === 1 || charBeforeSymbol === " " || charBeforeSymbol === "\t") {
          this.tryTriggerAutocomplete();
        }
      }
    } else {
      this.updateAutocomplete();
    }
  }

  private addNewLine(): void {
    this.cancelAutocomplete();
    this.historyIndex = -1; // Exit history browsing mode
    this.lastAction = null;

    this.pushUndoSnapshot();

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    const before = currentLine.slice(0, this.state.cursorCol);
    const after = currentLine.slice(this.state.cursorCol);

    // Split current line
    this.state.lines[this.state.cursorLine] = before;
    this.state.lines.splice(this.state.cursorLine + 1, 0, after);

    // Move cursor to start of new line
    this.state.cursorLine++;
    this.setCursorCol(0);

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private submitValue(): void {
    this.cancelAutocomplete();
    const result = this.state.lines.join("\n").trim();

    this.state = { lines: [""], cursorLine: 0, cursorCol: 0 };
    this.historyIndex = -1;
    this.scrollOffset = 0;
    this.undoStack.clear();
    this.lastAction = null;

    if (this.onChange) this.onChange("");
    if (this.onSubmit) this.onSubmit(result);
  }

  private handleBackspace(): void {
    this.historyIndex = -1; // Exit history browsing mode
    this.lastAction = null;

    if (this.state.cursorCol > 0) {
      this.pushUndoSnapshot();

      // Delete grapheme before cursor (handles emojis, combining characters, etc.)
      const line = this.state.lines[this.state.cursorLine] || "";
      const beforeCursor = line.slice(0, this.state.cursorCol);

      // Find the last grapheme in the text before cursor
      const graphemes = [...baseSegmenter.segment(beforeCursor)];
      const lastGrapheme = graphemes[graphemes.length - 1];
      const graphemeLength = lastGrapheme ? lastGrapheme.segment.length : 1;

      const before = line.slice(0, this.state.cursorCol - graphemeLength);
      const after = line.slice(this.state.cursorCol);

      this.state.lines[this.state.cursorLine] = before + after;
      this.setCursorCol(this.state.cursorCol - graphemeLength);
    } else if (this.state.cursorLine > 0) {
      this.pushUndoSnapshot();

      // Merge with previous line
      const currentLine = this.state.lines[this.state.cursorLine] || "";
      const previousLine = this.state.lines[this.state.cursorLine - 1] || "";

      this.state.lines[this.state.cursorLine - 1] = previousLine + currentLine;
      this.state.lines.splice(this.state.cursorLine, 1);

      this.state.cursorLine--;
      this.setCursorCol(previousLine.length);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  /**
   * Set cursor column and clear preferredVisualCol.
   * Use this for all non-vertical cursor movements to reset sticky column behavior.
   */
  private setCursorCol(col: number): void {
    this.state.cursorCol = col;
    this.preferredVisualCol = null;
  }

  private moveToLineStart(): void {
    this.lastAction = null;
    this.setCursorCol(0);
  }

  private moveToLineEnd(): void {
    this.lastAction = null;
    const currentLine = this.state.lines[this.state.cursorLine] || "";
    this.setCursorCol(currentLine.length);
  }

  private deleteToStartOfLine(): void {
    this.historyIndex = -1; // Exit history browsing mode

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    if (this.state.cursorCol > 0) {
      this.pushUndoSnapshot();

      // Calculate text to be deleted and save to kill ring (backward deletion = prepend)
      const deletedText = currentLine.slice(0, this.state.cursorCol);
      this.killRing.push(deletedText, { prepend: true, accumulate: this.lastAction === "kill" });
      this.lastAction = "kill";

      // Delete from start of line up to cursor
      this.state.lines[this.state.cursorLine] = currentLine.slice(this.state.cursorCol);
      this.setCursorCol(0);
    } else if (this.state.cursorLine > 0) {
      this.pushUndoSnapshot();

      // At start of line - merge with previous line, treating newline as deleted text
      this.killRing.push("\n", { prepend: true, accumulate: this.lastAction === "kill" });
      this.lastAction = "kill";

      const previousLine = this.state.lines[this.state.cursorLine - 1] || "";
      this.state.lines[this.state.cursorLine - 1] = previousLine + currentLine;
      this.state.lines.splice(this.state.cursorLine, 1);
      this.state.cursorLine--;
      this.setCursorCol(previousLine.length);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private deleteToEndOfLine(): void {
    this.historyIndex = -1; // Exit history browsing mode

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    if (this.state.cursorCol < currentLine.length) {
      this.pushUndoSnapshot();

      // Calculate text to be deleted and save to kill ring (forward deletion = append)
      const deletedText = currentLine.slice(this.state.cursorCol);
      this.killRing.push(deletedText, { prepend: false, accumulate: this.lastAction === "kill" });
      this.lastAction = "kill";

      // Delete from cursor to end of line
      this.state.lines[this.state.cursorLine] = currentLine.slice(0, this.state.cursorCol);
    } else if (this.state.cursorLine < this.state.lines.length - 1) {
      this.pushUndoSnapshot();

      // At end of line - merge with next line, treating newline as deleted text
      this.killRing.push("\n", { prepend: false, accumulate: this.lastAction === "kill" });
      this.lastAction = "kill";

      const nextLine = this.state.lines[this.state.cursorLine + 1] || "";
      this.state.lines[this.state.cursorLine] = currentLine + nextLine;
      this.state.lines.splice(this.state.cursorLine + 1, 1);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private deleteWordBackwards(): void {
    this.historyIndex = -1; // Exit history browsing mode

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    // If at start of line, behave like backspace at column 0 (merge with previous line)
    if (this.state.cursorCol === 0) {
      if (this.state.cursorLine > 0) {
        this.pushUndoSnapshot();

        // Treat newline as deleted text (backward deletion = prepend)
        this.killRing.push("\n", { prepend: true, accumulate: this.lastAction === "kill" });
        this.lastAction = "kill";

        const previousLine = this.state.lines[this.state.cursorLine - 1] || "";
        this.state.lines[this.state.cursorLine - 1] = previousLine + currentLine;
        this.state.lines.splice(this.state.cursorLine, 1);
        this.state.cursorLine--;
        this.setCursorCol(previousLine.length);
      }
    } else {
      this.pushUndoSnapshot();

      // Save lastAction before cursor movement (moveWordBackwards resets it)
      const wasKill = this.lastAction === "kill";

      const oldCursorCol = this.state.cursorCol;
      this.moveWordBackwards();
      const deleteFrom = this.state.cursorCol;
      this.setCursorCol(oldCursorCol);

      const deletedText = currentLine.slice(deleteFrom, this.state.cursorCol);
      this.killRing.push(deletedText, { prepend: true, accumulate: wasKill });
      this.lastAction = "kill";

      this.state.lines[this.state.cursorLine] =
        currentLine.slice(0, deleteFrom) + currentLine.slice(this.state.cursorCol);
      this.setCursorCol(deleteFrom);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private deleteWordForward(): void {
    this.historyIndex = -1; // Exit history browsing mode

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    // If at end of line, merge with next line (delete the newline)
    if (this.state.cursorCol >= currentLine.length) {
      if (this.state.cursorLine < this.state.lines.length - 1) {
        this.pushUndoSnapshot();

        // Treat newline as deleted text (forward deletion = append)
        this.killRing.push("\n", { prepend: false, accumulate: this.lastAction === "kill" });
        this.lastAction = "kill";

        const nextLine = this.state.lines[this.state.cursorLine + 1] || "";
        this.state.lines[this.state.cursorLine] = currentLine + nextLine;
        this.state.lines.splice(this.state.cursorLine + 1, 1);
      }
    } else {
      this.pushUndoSnapshot();

      // Save lastAction before cursor movement (moveWordForwards resets it)
      const wasKill = this.lastAction === "kill";

      const oldCursorCol = this.state.cursorCol;
      this.moveWordForwards();
      const deleteTo = this.state.cursorCol;
      this.setCursorCol(oldCursorCol);

      const deletedText = currentLine.slice(this.state.cursorCol, deleteTo);
      this.killRing.push(deletedText, { prepend: false, accumulate: wasKill });
      this.lastAction = "kill";

      this.state.lines[this.state.cursorLine] =
        currentLine.slice(0, this.state.cursorCol) + currentLine.slice(deleteTo);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private handleForwardDelete(): void {
    this.historyIndex = -1; // Exit history browsing mode
    this.lastAction = null;

    const currentLine = this.state.lines[this.state.cursorLine] || "";

    if (this.state.cursorCol < currentLine.length) {
      this.pushUndoSnapshot();

      // Delete grapheme at cursor position (handles emojis, combining characters, etc.)
      const afterCursor = currentLine.slice(this.state.cursorCol);

      // Find the first grapheme at cursor
      const graphemes = [...baseSegmenter.segment(afterCursor)];
      const firstGrapheme = graphemes[0];
      const graphemeLength = firstGrapheme ? firstGrapheme.segment.length : 1;

      const before = currentLine.slice(0, this.state.cursorCol);
      const after = currentLine.slice(this.state.cursorCol + graphemeLength);
      this.state.lines[this.state.cursorLine] = before + after;
    } else if (this.state.cursorLine < this.state.lines.length - 1) {
      this.pushUndoSnapshot();

      // At end of line - merge with next line
      const nextLine = this.state.lines[this.state.cursorLine + 1] || "";
      this.state.lines[this.state.cursorLine] = currentLine + nextLine;
      this.state.lines.splice(this.state.cursorLine + 1, 1);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  /**
   * Build a mapping from visual lines to logical positions.
   * Returns an array where each element represents a visual line with:
   * - logicalLine: index into this.state.lines
   * - startCol: starting column in the logical line
   * - length: length of this visual line segment
   */
  private buildVisualLineMap(width: number): Array<{ logicalLine: number; startCol: number; length: number }> {
    const visualLines: Array<{ logicalLine: number; startCol: number; length: number }> = [];

    for (let i = 0; i < this.state.lines.length; i++) {
      const line = this.state.lines[i] || "";
      const lineVisWidth = visibleWidth(line);
      if (line.length === 0) {
        // Empty line still takes one visual line
        visualLines.push({ logicalLine: i, startCol: 0, length: 0 });
      } else if (lineVisWidth <= width) {
        visualLines.push({ logicalLine: i, startCol: 0, length: line.length });
      } else {
        // Line needs wrapping - use word-aware wrapping
        const chunks = wordWrapLine(line, width);
        for (const chunk of chunks) {
          visualLines.push({
            logicalLine: i,
            startCol: chunk.startIndex,
            length: chunk.endIndex - chunk.startIndex,
          });
        }
      }
    }

    return visualLines;
  }

  /**
   * Find the visual line index that contains the given logical position.
   */
  private findVisualLineAt(
    visualLines: Array<{ logicalLine: number; startCol: number; length: number }>,
    line: number,
    col: number
  ): number {
    for (let i = 0; i < visualLines.length; i++) {
      const vl = visualLines[i];
      if (!vl || vl.logicalLine !== line) continue;
      const offset = col - vl.startCol;
      // Cursor is in this segment if it's within range. For the last
      // segment of a logical line, cursor can be at length (end position)
      const isLastSegmentOfLine = i === visualLines.length - 1 || visualLines[i + 1]?.logicalLine !== vl.logicalLine;
      if (offset >= 0 && (offset < vl.length || (isLastSegmentOfLine && offset === vl.length))) {
        return i;
      }
    }
    return visualLines.length - 1;
  }

  /**
   * Find the visual line index for the current cursor position.
   */
  private findCurrentVisualLine(
    visualLines: Array<{ logicalLine: number; startCol: number; length: number }>
  ): number {
    return this.findVisualLineAt(visualLines, this.state.cursorLine, this.state.cursorCol);
  }

  private moveCursor(deltaLine: number, deltaCol: number): void {
    this.lastAction = null;
    const visualLines = this.buildVisualLineMap(this.lastWidth);
    const currentVisualLine = this.findCurrentVisualLine(visualLines);

    if (deltaLine !== 0) {
      const targetVisualLine = currentVisualLine + deltaLine;

      if (targetVisualLine >= 0 && targetVisualLine < visualLines.length) {
        this.moveToVisualLine(visualLines, currentVisualLine, targetVisualLine);
      }
    }

    if (deltaCol !== 0) {
      const currentLine = this.state.lines[this.state.cursorLine] || "";

      if (deltaCol > 0) {
        // Moving right - move by one grapheme (handles emojis, combining characters, etc.)
        if (this.state.cursorCol < currentLine.length) {
          const afterCursor = currentLine.slice(this.state.cursorCol);
          const graphemes = [...baseSegmenter.segment(afterCursor)];
          const firstGrapheme = graphemes[0];
          this.setCursorCol(this.state.cursorCol + (firstGrapheme ? firstGrapheme.segment.length : 1));
        } else if (this.state.cursorLine < this.state.lines.length - 1) {
          // Wrap to start of next logical line
          this.state.cursorLine++;
          this.setCursorCol(0);
        } else {
          // At end of last line - can't move, but set preferredVisualCol for up/down navigation
          const currentVL = visualLines[currentVisualLine];
          if (currentVL) {
            this.preferredVisualCol = this.state.cursorCol - currentVL.startCol;
          }
        }
      } else {
        // Moving left - move by one grapheme (handles emojis, combining characters, etc.)
        if (this.state.cursorCol > 0) {
          const beforeCursor = currentLine.slice(0, this.state.cursorCol);
          const graphemes = [...baseSegmenter.segment(beforeCursor)];
          const lastGrapheme = graphemes[graphemes.length - 1];
          this.setCursorCol(this.state.cursorCol - (lastGrapheme ? lastGrapheme.segment.length : 1));
        } else if (this.state.cursorLine > 0) {
          // Wrap to end of previous logical line
          this.state.cursorLine--;
          const prevLine = this.state.lines[this.state.cursorLine] || "";
          this.setCursorCol(prevLine.length);
        }
      }
    }
  }

  /**
   * Move cursor to a target visual line, applying sticky column logic.
   * Shared by moveCursor() and pageScroll().
   */
  private moveToVisualLine(
    visualLines: Array<{ logicalLine: number; startCol: number; length: number }>,
    currentVisualLine: number,
    targetVisualLine: number
  ): void {
    const currentVL = visualLines[currentVisualLine];
    const targetVL = visualLines[targetVisualLine];
    if (!(currentVL && targetVL)) return;

    let currentVisualCol: number;
    if (this.preferredVisualCol !== null) {
      currentVisualCol = this.preferredVisualCol;
    } else {
      currentVisualCol = this.state.cursorCol - currentVL.startCol;
    }

    // Compute target visual column using sticky column logic
    const sourceMaxVisualCol = currentVL.length;
    const targetMaxVisualCol = targetVL.length;
    const moveToVisualCol = this.computeVerticalMoveColumn(currentVisualCol, sourceMaxVisualCol, targetMaxVisualCol);

    // Set cursor position
    this.state.cursorLine = targetVL.logicalLine;
    const targetCol = targetVL.startCol + moveToVisualCol;
    const logicalLine = this.state.lines[targetVL.logicalLine] || "";
    this.state.cursorCol = Math.min(targetCol, logicalLine.length);
  }

  /**
   * Compute the target visual column for vertical cursor movement.
   * Implements the sticky column decision table.
   */
  private computeVerticalMoveColumn(
    currentVisualCol: number,
    sourceMaxVisualCol: number,
    targetMaxVisualCol: number
  ): number {
    const hasPreferred = this.preferredVisualCol !== null; // P
    const cursorInMiddle = currentVisualCol < sourceMaxVisualCol; // S
    const targetTooShort = targetMaxVisualCol < currentVisualCol; // T

    if (!hasPreferred || cursorInMiddle) {
      if (targetTooShort) {
        // Cases 2 and 7
        this.preferredVisualCol = currentVisualCol;
        return targetMaxVisualCol;
      }

      // Cases 1 and 6
      this.preferredVisualCol = null;
      return currentVisualCol;
    }

    const targetCantFitPreferred = targetMaxVisualCol < this.preferredVisualCol!; // U
    if (targetTooShort || targetCantFitPreferred) {
      // Cases 4 and 5
      return targetMaxVisualCol;
    }

    // Case 3
    const result = this.preferredVisualCol!;
    this.preferredVisualCol = null;
    return result;
  }

  /**
   * Scroll by a page (direction: -1 for up, 1 for down).
   * Moves cursor by the page size while keeping it in bounds.
   */
  private pageScroll(direction: -1 | 1): void {
    this.lastAction = null;
    const terminalRows = this.tui.terminal.rows;
    const pageSize = Math.max(5, Math.floor(terminalRows * 0.3));

    const visualLines = this.buildVisualLineMap(this.lastWidth);
    const currentVisualLine = this.findCurrentVisualLine(visualLines);
    const targetVisualLine = Math.max(0, Math.min(visualLines.length - 1, currentVisualLine + direction * pageSize));

    this.moveToVisualLine(visualLines, currentVisualLine, targetVisualLine);
  }

  private moveWordBackwards(): void {
    this.lastAction = null;
    const currentLine = this.state.lines[this.state.cursorLine] || "";

    // If at start of line, move to end of previous line
    if (this.state.cursorCol === 0) {
      if (this.state.cursorLine > 0) {
        this.state.cursorLine--;
        const prevLine = this.state.lines[this.state.cursorLine] || "";
        this.setCursorCol(prevLine.length);
      }
      return;
    }

    const textBeforeCursor = currentLine.slice(0, this.state.cursorCol);
    const graphemes = [...baseSegmenter.segment(textBeforeCursor)];
    let newCol = this.state.cursorCol;

    // Skip trailing whitespace
    while (
      graphemes.length > 0 &&
      isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || "")
    ) {
      newCol -= graphemes.pop()?.segment.length || 0;
    }

    if (graphemes.length > 0) {
      const lastGrapheme = graphemes[graphemes.length - 1]?.segment || "";
      if (isPunctuationChar(lastGrapheme)) {
        // Skip punctuation run
        while (
          graphemes.length > 0 &&
          isPunctuationChar(graphemes[graphemes.length - 1]?.segment || "")
        ) {
          newCol -= graphemes.pop()?.segment.length || 0;
        }
      } else {
        // Skip word run
        while (
          graphemes.length > 0 &&
          !isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || "") &&
          !isPunctuationChar(graphemes[graphemes.length - 1]?.segment || "")
        ) {
          newCol -= graphemes.pop()?.segment.length || 0;
        }
      }
    }

    this.setCursorCol(newCol);
  }

  private moveWordForwards(): void {
    this.lastAction = null;
    const currentLine = this.state.lines[this.state.cursorLine] || "";

    // If at end of line, move to start of next line
    if (this.state.cursorCol >= currentLine.length) {
      if (this.state.cursorLine < this.state.lines.length - 1) {
        this.state.cursorLine++;
        this.setCursorCol(0);
      }
      return;
    }

    const textAfterCursor = currentLine.slice(this.state.cursorCol);
    const segments = [...baseSegmenter.segment(textAfterCursor)];
    let newCol = this.state.cursorCol;
    let idx = 0;

    // Skip leading whitespace
    while (idx < segments.length && isWhitespaceChar(segments[idx]?.segment || "")) {
      newCol += segments[idx]?.segment.length || 0;
      idx++;
    }

    if (idx < segments.length) {
      const firstGrapheme = segments[idx]?.segment || "";
      if (isPunctuationChar(firstGrapheme)) {
        // Skip punctuation run
        while (idx < segments.length && isPunctuationChar(segments[idx]?.segment || "")) {
          newCol += segments[idx]?.segment.length || 0;
          idx++;
        }
      } else {
        // Skip word run
        while (
          idx < segments.length &&
          !isWhitespaceChar(segments[idx]?.segment || "") &&
          !isPunctuationChar(segments[idx]?.segment || "")
        ) {
          newCol += segments[idx]?.segment.length || 0;
          idx++;
        }
      }
    }

    this.setCursorCol(newCol);
  }

  /**
   * Yank (paste) the most recent kill ring entry at cursor position.
   */
  private yank(): void {
    if (this.killRing.length === 0) return;

    this.pushUndoSnapshot();

    const text = this.killRing.peek()!;
    this.insertYankedText(text);

    this.lastAction = "yank";
  }

  /**
   * Cycle through kill ring (only works immediately after yank or yank-pop).
   * Replaces the last yanked text with the previous entry in the ring.
   */
  private yankPop(): void {
    // Only works if we just yanked and have more than one entry
    if (this.lastAction !== "yank" || this.killRing.length <= 1) return;

    this.pushUndoSnapshot();

    // Delete the previously yanked text (still at end of ring before rotation)
    this.deleteYankedText();

    // Rotate the ring: move end to front
    this.killRing.rotate();

    // Insert the new most recent entry (now at end after rotation)
    const text = this.killRing.peek()!;
    this.insertYankedText(text);

    this.lastAction = "yank";
  }

  /**
   * Insert text at cursor position (used by yank operations).
   */
  private insertYankedText(text: string): void {
    this.historyIndex = -1; // Exit history browsing mode
    const lines = text.split("\n");

    if (lines.length === 1) {
      // Single line - insert at cursor
      const currentLine = this.state.lines[this.state.cursorLine] || "";
      const before = currentLine.slice(0, this.state.cursorCol);
      const after = currentLine.slice(this.state.cursorCol);
      this.state.lines[this.state.cursorLine] = before + text + after;
      this.setCursorCol(this.state.cursorCol + text.length);
    } else {
      // Multi-line insert
      const currentLine = this.state.lines[this.state.cursorLine] || "";
      const before = currentLine.slice(0, this.state.cursorCol);
      const after = currentLine.slice(this.state.cursorCol);

      // First line merges with text before cursor
      this.state.lines[this.state.cursorLine] = before + (lines[0] || "");

      // Insert middle lines
      for (let i = 1; i < lines.length - 1; i++) {
        this.state.lines.splice(this.state.cursorLine + i, 0, lines[i] || "");
      }

      // Last line merges with text after cursor
      const lastLineIndex = this.state.cursorLine + lines.length - 1;
      this.state.lines.splice(lastLineIndex, 0, (lines[lines.length - 1] || "") + after);

      // Update cursor position
      this.state.cursorLine = lastLineIndex;
      this.setCursorCol((lines[lines.length - 1] || "").length);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  /**
   * Delete the previously yanked text (used by yank-pop).
   */
  private deleteYankedText(): void {
    const yankedText = this.killRing.peek();
    if (!yankedText) return;

    const yankLines = yankedText.split("\n");

    if (yankLines.length === 1) {
      // Single line - delete backward from cursor
      const currentLine = this.state.lines[this.state.cursorLine] || "";
      const deleteLen = yankedText.length;
      const before = currentLine.slice(0, this.state.cursorCol - deleteLen);
      const after = currentLine.slice(this.state.cursorCol);
      this.state.lines[this.state.cursorLine] = before + after;
      this.setCursorCol(this.state.cursorCol - deleteLen);
    } else {
      // Multi-line delete - cursor is at end of last yanked line
      const startLine = this.state.cursorLine - (yankLines.length - 1);
      const startCol = (this.state.lines[startLine] || "").length - (yankLines[0] || "").length;

      // Get text after cursor on current line
      const afterCursor = (this.state.lines[this.state.cursorLine] || "").slice(this.state.cursorCol);

      // Get text before yank start position
      const beforeYank = (this.state.lines[startLine] || "").slice(0, startCol);

      // Remove all lines from startLine to cursorLine and replace with merged line
      this.state.lines.splice(startLine, yankLines.length, beforeYank + afterCursor);

      // Update cursor
      this.state.cursorLine = startLine;
      this.setCursorCol(startCol);
    }

    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  private pushUndoSnapshot(): void {
    this.undoStack.push(this.state);
  }

  private undo(): void {
    this.historyIndex = -1; // Exit history browsing mode
    const snapshot = this.undoStack.pop();
    if (!snapshot) return;
    Object.assign(this.state, snapshot);
    this.lastAction = null;
    this.preferredVisualCol = null;
    if (this.onChange) {
      this.onChange(this.getText());
    }
  }

  // Helper method to check if cursor is at start of message (for slash command detection)
  private isAtStartOfMessage(): boolean {
    const currentLine = this.state.lines[this.state.cursorLine] || "";
    const beforeCursor = currentLine.slice(0, this.state.cursorCol);
    return beforeCursor.trim() === "" || beforeCursor.trim() === "/";
  }

  // Autocomplete methods
  private tryTriggerAutocomplete(): void {
    if (!this.autocompleteProvider) return;
    this.requestAutocomplete(false);
  }

  private handleTabCompletion(): void {
    if (!this.autocompleteProvider) return;

    const currentLine = this.state.lines[this.state.cursorLine] || "";
    const beforeCursor = currentLine.slice(0, this.state.cursorCol);

    if (beforeCursor.trimStart().startsWith("/") && !beforeCursor.trimStart().includes(" ")) {
      this.handleSlashCommandCompletion();
    } else {
      this.forceFileAutocomplete(true);
    }
  }

  private handleSlashCommandCompletion(): void {
    this.requestAutocomplete(false);
  }

  private forceFileAutocomplete(_explicitTab: boolean = false): void {
    this.requestAutocomplete(true);
  }

  private requestAutocomplete(force: boolean): void {
    if (!this.autocompleteProvider) return;

    this.cancelAutocompleteRequest();
    const startToken = ++this.autocompleteStartToken;

    // Detect if this is a slash command completion
    const currentLine = this.state.lines[this.state.cursorLine] || "";
    const beforeCursor = currentLine.slice(0, this.state.cursorCol);
    this.autocompleteIsSlashCommand = beforeCursor.trimStart().startsWith("/") && !beforeCursor.trimStart().includes(" ");

    void this.startAutocompleteRequest(startToken, force);
  }

  private async startAutocompleteRequest(startToken: number, force: boolean): Promise<void> {
    const previousTask = this.autocompleteRequestTask;
    this.autocompleteRequestTask = (async () => {
      await previousTask;
      if (startToken !== this.autocompleteStartToken || !this.autocompleteProvider) {
        return;
      }

      const controller = new AbortController();
      this.autocompleteAbort = controller;
      const requestId = ++this.autocompleteRequestId;
      const snapshotText = this.getText();
      const snapshotLine = this.state.cursorLine;
      const snapshotCol = this.state.cursorCol;

      await this.runAutocompleteRequest(requestId, controller, snapshotText, snapshotLine, snapshotCol, force);
    })();
    await this.autocompleteRequestTask;
  }

  private async runAutocompleteRequest(
    requestId: number,
    controller: AbortController,
    snapshotText: string,
    snapshotLine: number,
    snapshotCol: number,
    force: boolean
  ): Promise<void> {
    if (!this.autocompleteProvider) return;

    const currentText = this.getText();
    const suggestions = await this.autocompleteProvider.getSuggestions(currentText, snapshotCol);

    if (!this.isAutocompleteRequestCurrent(requestId, controller, snapshotText, snapshotLine, snapshotCol)) {
      return;
    }

    this.autocompleteAbort = undefined;

    if (!suggestions || !Array.isArray(suggestions.items) || suggestions.items.length === 0) {
      this.cancelAutocomplete();
      this.tui.requestRender();
      return;
    }

    this.applyAutocompleteSuggestions(suggestions, force ? "force" : "regular");
    this.tui.requestRender();
  }

  private isAutocompleteRequestCurrent(
    requestId: number,
    controller: AbortController,
    snapshotText: string,
    snapshotLine: number,
    snapshotCol: number
  ): boolean {
    return (
      !controller.signal.aborted &&
      requestId === this.autocompleteRequestId &&
      this.getText() === snapshotText &&
      this.state.cursorLine === snapshotLine &&
      this.state.cursorCol === snapshotCol
    );
  }

  private applyAutocompleteSuggestions(suggestions: AutocompleteSuggestions, state: "regular" | "force"): void {
    const items = suggestions.items
      .filter((item): item is { value: string; label: string; description?: string } => item.value !== undefined && item.label !== undefined)
      .map(item => ({ value: item.value!, label: item.label!, description: item.description }));
    this.autocompleteList = this.createAutocompleteList(items);
    this.autocompleteState = state;
  }

  private createAutocompleteList(items: Array<{ value: string; label: string; description?: string }>): SelectList {
    const layout = this.autocompleteIsSlashCommand ? SLASH_COMMAND_SELECT_LIST_LAYOUT : undefined;
    return new SelectList(items, this.autocompleteMaxVisible, this.theme.selectList, layout);
  }

  private cancelAutocompleteRequest(): void {
    this.autocompleteStartToken += 1;
    if (this.autocompleteDebounceTimer) {
      clearTimeout(this.autocompleteDebounceTimer);
      this.autocompleteDebounceTimer = undefined;
    }
    this.autocompleteAbort?.abort();
    this.autocompleteAbort = undefined;
  }

  private cancelAutocomplete(): void {
    this.cancelAutocompleteRequest();
    this.autocompleteState = null;
    this.autocompleteList = undefined;
    this.autocompleteIsSlashCommand = false;
  }

  public isShowingAutocomplete(): boolean {
    return this.autocompleteState !== null;
  }

  private updateAutocomplete(): void {
    if (!this.autocompleteState || !this.autocompleteProvider) return;
    this.requestAutocomplete(this.autocompleteState === "force");
  }
}
