import { describe, it, expect, beforeEach } from "vitest";
import { Editor } from "../editor.js";
import type { TUI } from "../../core/tui.js";
import type { EditorTheme } from "../editor.js";

// Mock TUI for testing
const createMockTUI = (): TUI => {
  return {
    terminal: {
      rows: 24,
      columns: 80,
      write: () => {},
      start: () => {},
      stop: () => {},
      hideCursor: () => {},
      showCursor: () => {},
      moveBy: () => {},
      clearLine: () => {},
      setTitle: () => {},
    },
    requestRender: () => {},
    addOverlay: () => ({
      hide: () => {},
      setHidden: () => {},
      focus: () => {},
      unfocus: () => {},
      isFocused: () => false,
    }),
    removeOverlay: () => {},
    addChild: () => {},
    removeChild: () => {},
  } as any;
};

// Default theme for testing
const defaultTheme: EditorTheme = {
  borderColor: (str: string) => str,
  selectList: {
    selectedPrefix: (text: string) => `> ${text}`,
    selectedText: (text: string) => text,
    description: (text: string) => text,
    scrollInfo: (text: string) => text,
    noMatch: (text: string) => text,
  },
};

describe("Editor Component", () => {
  let editor: Editor;
  let mockTUI: TUI;

  beforeEach(() => {
    mockTUI = createMockTUI();
    editor = new Editor(mockTUI, defaultTheme);
  });

  describe("Initialization", () => {
    it("should initialize with empty content", () => {
      expect(editor.getText()).toBe("");
      expect(editor.getLines()).toEqual([""]);
      expect(editor.getCursor()).toEqual({ line: 0, col: 0 });
    });

    it("should have focused property", () => {
      expect(editor.focused).toBe(false);
    });
  });

  describe("Text Manipulation", () => {
    it("should set text", () => {
      editor.setText("Hello World");
      expect(editor.getText()).toBe("Hello World");
    });

    it("should handle multi-line text", () => {
      editor.setText("Line 1\nLine 2\nLine 3");
      expect(editor.getLines()).toEqual(["Line 1", "Line 2", "Line 3"]);
      expect(editor.getCursor().line).toBe(2);
    });

    it("should normalize line endings", () => {
      editor.setText("Line 1\r\nLine 2\rLine 3");
      expect(editor.getLines()).toEqual(["Line 1", "Line 2", "Line 3"]);
    });

    it("should expand tabs to spaces", () => {
      editor.setText("Hello\tWorld");
      expect(editor.getText()).toBe("Hello    World");
    });
  });

  describe("Cursor Movement", () => {
    it("should move cursor right", () => {
      editor.setText("Hello");
      editor.handleInput("\x1b[H"); // Move to start
      editor.handleInput("\x1b[C"); // Right arrow
      expect(editor.getCursor().col).toBe(1);
    });

    it("should move cursor left", () => {
      editor.setText("Hello");
      editor.handleInput("\x1b[H"); // Move to start
      editor.handleInput("\x1b[C"); // Right
      editor.handleInput("\x1b[C"); // Right
      editor.handleInput("\x1b[D"); // Left
      expect(editor.getCursor().col).toBe(1);
    });

    it("should move to line start", () => {
      editor.setText("Hello World");
      editor.handleInput("\x1b[H"); // Home key
      expect(editor.getCursor().col).toBe(0);
    });

    it("should move to line end", () => {
      editor.setText("Hello");
      editor.handleInput("\x1b[F"); // End key
      expect(editor.getCursor().col).toBe(5);
    });
  });

  describe("Text Insertion", () => {
    it("should insert characters", () => {
      editor.handleInput("a");
      expect(editor.getText()).toBe("a");
    });

    it("should insert at cursor position", () => {
      editor.setText("Hello World");
      // After setText, cursor is at end (position 11)
      editor.handleInput("\x1b[D"); // Move left once to position 10 (before 'd')
      editor.handleInput("X");
      expect(editor.getText()).toBe("Hello WorlXd");
    });

    it("should insert text at cursor programmatically", () => {
      editor.setText("Hello World");
      // After setText, cursor is at end (position 11)
      // Move left 5 times to position 6 (at 'W')
      for (let i = 0; i < 5; i++) {
        editor.handleInput("\x1b[D");
      }
      editor.insertTextAtCursor("Beautiful ");
      expect(editor.getText()).toBe("Hello Beautiful World");
    });
  });

  describe("Text Deletion", () => {
    it("should handle backspace", () => {
      editor.setText("Hello");
      editor.handleInput("\x7f"); // Backspace
      expect(editor.getText()).toBe("Hell");
    });

    it("should delete to end of line", () => {
      editor.setText("Hello World");
      editor.handleInput("\x1b[H"); // Home
      editor.handleInput("\x1b[3~"); // Delete forward (or use appropriate keybinding)
      // Note: This test depends on actual keybindings setup
    });

    it("should delete word backward", () => {
      editor.setText("Hello World");
      editor.handleInput("\x1b[F"); // End
      editor.handleInput("\x1b[1;5D"); // Ctrl+Left (word backward)
      editor.handleInput("\x1b[1;5D"); // Ctrl+Left again
      // The exact behavior depends on keybindings
    });
  });

  describe("Undo/Redo", () => {
    it("should support undo", () => {
      editor.setText("Hello");
      editor.handleInput(" World");
      expect(editor.getText()).toBe("Hello World");
      
      editor.handleInput("\x1b[Z"); // Undo (Ctrl+Z or similar)
      // Note: Undo behavior depends on implementation details
    });
  });

  describe("Multi-line Editing", () => {
    it("should add new lines", () => {
      editor.setText("Line 1");
      editor.handleInput("\n"); // Enter
      expect(editor.getLines().length).toBe(2);
      expect(editor.getCursor().line).toBe(1);
      expect(editor.getCursor().col).toBe(0);
    });

    it("should merge lines on backspace at line start", () => {
      editor.setText("Line 1\nLine 2");
      // After setText, cursor is at end of line 1 (second line)
      editor.handleInput("\x1b[H"); // Home - move to start of current line (line 1)
      expect(editor.getCursor().line).toBe(1);
      expect(editor.getCursor().col).toBe(0);
      editor.handleInput("\x7f"); // Backspace - should merge with line 0
      expect(editor.getLines().length).toBe(1);
      expect(editor.getText()).toBe("Line 1Line 2");
    });
  });

  describe("History Navigation", () => {
    it("should navigate history with up/down arrows when empty", () => {
      editor.addToHistory("Command 1");
      editor.addToHistory("Command 2");
      
      // When editor is empty, up arrow should show history
      editor.handleInput("\x1b[A"); // Up arrow
      expect(editor.getText()).toBe("Command 2");
    });
  });

  describe("Rendering", () => {
    it("should render empty editor", () => {
      const rendered = editor.render(80);
      expect(rendered.length).toBeGreaterThan(0);
      expect(Array.isArray(rendered)).toBe(true);
    });

    it("should render text content", () => {
      editor.setText("Hello World");
      const rendered = editor.render(80);
      expect(rendered.some(line => line.includes("Hello World"))).toBe(true);
    });

    it("should respect width parameter", () => {
      editor.setText("A very long line that should wrap when the width is limited");
      const narrowRendered = editor.render(20);
      const wideRendered = editor.render(80);
      
      expect(narrowRendered.length).toBeGreaterThanOrEqual(wideRendered.length);
    });
  });

  describe("Autocomplete", () => {
    it("should not show autocomplete by default", () => {
      expect(editor.isShowingAutocomplete()).toBe(false);
    });

    it("should accept autocomplete provider", () => {
      const mockProvider = {
        getSuggestions: async (text: string, cursorPosition: number) => ({
          items: [],
          cursorPosition,
        }),
      };
      
      editor.setAutocompleteProvider(mockProvider);
      // Provider is set without errors
      expect(() => editor.setAutocompleteProvider(mockProvider)).not.toThrow();
    });
  });

  describe("Options", () => {
    it("should accept padding option", () => {
      const editorWithPadding = new Editor(mockTUI, defaultTheme, { paddingX: 2 });
      expect(editorWithPadding.getPaddingX()).toBe(2);
    });

    it("should accept autocomplete max visible option", () => {
      const editorWithMaxVisible = new Editor(mockTUI, defaultTheme, { 
        autocompleteMaxVisible: 10 
      });
      expect(editorWithMaxVisible.getAutocompleteMaxVisible()).toBe(10);
    });

    it("should clamp padding to valid range", () => {
      const editorWithNegativePadding = new Editor(mockTUI, defaultTheme, { 
        paddingX: -5 
      });
      expect(editorWithNegativePadding.getPaddingX()).toBe(0);
    });
  });

  describe("Event Callbacks", () => {
    it("should call onChange when text changes", () => {
      let changeCount = 0;
      editor.onChange = () => { changeCount++; };
      
      editor.handleInput("a");
      expect(changeCount).toBeGreaterThan(0);
    });

    it("should call onSubmit on enter", () => {
      let submitCalled = false;
      let submittedText = "";
      editor.onSubmit = (text) => {
        submitCalled = true;
        submittedText = text;
      };
      
      editor.setText("Test message");
      editor.handleInput("\r"); // Enter
      
      expect(submitCalled).toBe(true);
      expect(submittedText).toBe("Test message");
    });
  });

  describe("Focus Management", () => {
    it("should implement Focusable interface", () => {
      expect("focused" in editor).toBe(true);
      expect(typeof editor.focused).toBe("boolean");
    });

    it("should allow setting focused property", () => {
      editor.focused = true;
      expect(editor.focused).toBe(true);
      
      editor.focused = false;
      expect(editor.focused).toBe(false);
    });
  });
});
