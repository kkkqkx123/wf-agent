export interface CanvasShortcutEntry {
	keys: string;
	action: string;
}

/**
 * Shared vocabulary for canvas shortcuts. The global help overlay renders
 * these rows, so renaming a shortcut only happens here.
 */
export const CANVAS_SHORTCUT_HELP: CanvasShortcutEntry[] = [
	{ keys: 'F', action: 'Fit graph to view' },
	{ keys: 'R', action: 'Re-run layout' },
	{ keys: 'E', action: 'Enter or exit edit mode' },
	{ keys: 'Ctrl/⌘ + S', action: 'Save draft' },
	{ keys: 'Ctrl/⌘ + Z / Ctrl/⌘ + Shift + Z', action: 'Undo / redo' },
	{ keys: 'Ctrl/⌘ + A', action: 'Select all nodes' },
	{ keys: 'Del', action: 'Delete selection' },
];

export interface CanvasShortcutHandlers {
	fit(): void;
	relayout(): void;
	selectAll(): void;
	deleteSelected(): void;
	undo(): void;
	redo(): void;
	toggleEdit(): void;
	save(): void;
	/** Whether write operations are allowed right now (edit mode). */
	canWrite(): boolean;
	/** Called when a write shortcut fires while read-only. */
	onreadonlywrite(): void;
}

function isTyping(target: HTMLElement | null): boolean {
	if (!target) return false;
	return (
		target.tagName === 'INPUT' ||
		target.tagName === 'TEXTAREA' ||
		target.tagName === 'SELECT' ||
		target.isContentEditable
	);
}

/**
 * Register canvas shortcuts on the window, scoped to focus inside `root`.
 * Typing in fields never triggers shortcuts. Write operations are gated by
 * `canWrite`; read-only presses report through `onreadonlywrite` without
 * side effects. Returns a dispose function.
 */
export function registerCanvasShortcuts(
	root: HTMLElement,
	get: () => CanvasShortcutHandlers,
): () => void {
	function guarded(
		handlers: CanvasShortcutHandlers,
		fn: (handlers: CanvasShortcutHandlers) => void,
	): void {
		if (!handlers.canWrite()) {
			handlers.onreadonlywrite();
			return;
		}
		fn(handlers);
	}
	function onKey(event: KeyboardEvent): void {
		const target = event.target as HTMLElement | null;
		if (isTyping(target)) return;
		if (!target || !root.contains(target)) return;
		const handlers = get();
		const mod = event.ctrlKey || event.metaKey;
		const key = event.key;
		if (mod && (key === 'z' || key === 'Z')) {
			event.preventDefault();
			if (event.shiftKey) {
				guarded(handlers, (h) => h.redo());
			} else {
				guarded(handlers, (h) => h.undo());
			}
			return;
		}
		if (mod && (key === 'a' || key === 'A')) {
			event.preventDefault();
			handlers.selectAll();
			return;
		}
		if (mod && (key === 's' || key === 'S')) {
			event.preventDefault();
			guarded(handlers, (h) => h.save());
			return;
		}
		if (mod) return;
		switch (key) {
			case 'Delete':
			case 'Backspace':
				event.preventDefault();
				guarded(handlers, (h) => h.deleteSelected());
				break;
			case 'f':
			case 'F':
				event.preventDefault();
				handlers.fit();
				break;
			case 'r':
			case 'R':
				event.preventDefault();
				handlers.relayout();
				break;
			case 'e':
			case 'E':
				event.preventDefault();
				handlers.toggleEdit();
				break;
			default:
				break;
		}
	}
	window.addEventListener('keydown', onKey);
	return () => window.removeEventListener('keydown', onKey);
}
