import { getHotkeyManager } from '@tanstack/hotkeys';
import {
	effectiveHotkey,
	isShortcutDisabled,
	resolvedShortcutRows,
} from '#lib/shortcuts/registry';

/**
 * Shared vocabulary for canvas shortcuts. The global help overlay renders
 * these rows, so renaming a shortcut only happens in the shortcut catalog.
 */
export const CANVAS_SHORTCUT_HELP: Array<{ keys: string; action: string }> =
	resolvedShortcutRows('canvas')
		.filter((row) => !row.disabled)
		.map((row) => ({ keys: row.keys, action: row.action.label }));

/**
 * Button label with its shortcut appended from the same catalog the help
 * overlay renders, so tooltips never drift from the binding.
 */
export function shortcutTitle(actionId: string, label: string): string {
	if (isShortcutDisabled(actionId)) return label;
	const keys = effectiveHotkey(actionId);
	return keys ? `${label} (${keys})` : label;
}

export interface CanvasShortcutHandlers {
	fit(): void;
	relayout(): void;
	zoomIn(): void;
	zoomOut(): void;
	focusSelected(): void;
	selectAll(): void;
	deleteSelected(): void;
	undo(): void;
	redo(): void;
	toggleEdit(): void;
	save(): void;
	nextNode(): void;
	prevNode(): void;
	toggleLogs(): void;
	toggleFilters(): void;
	/** Whether write operations are allowed right now (edit mode). */
	canWrite(): boolean;
	/** Called when a write shortcut fires while read-only. */
	onreadonlywrite(): void;
}

/**
 * Register canvas shortcuts from the shortcut catalog, scoped to focus
 * inside `root` (bindings attach to the root element, so events bubble
 * only from within it). Typing in fields never triggers shortcuts, except
 * Escape which exits edit mode even from inputs. Write operations are
 * gated by `canWrite`; read-only presses report through `onreadonlywrite`
 * without side effects. Zoom and focus are view operations and stay
 * available read-only. Returns a dispose function.
 */
export function registerCanvasShortcuts(
	root: HTMLElement,
	get: () => CanvasShortcutHandlers,
): () => void {
	const manager = getHotkeyManager();
	const handles: Array<{ unregister(): void }> = [];

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

	function bindWrite(
		actionId: string,
		run: (handlers: CanvasShortcutHandlers) => void,
	): void {
		if (isShortcutDisabled(actionId)) return;
		const hotkey = effectiveHotkey(actionId);
		if (!hotkey) return;
		handles.push(
			manager.register(
				hotkey,
				() => {
					const handlers = get();
					guarded(handlers, run);
				},
				{ target: root, ignoreInputs: true },
			),
		);
	}

	function bindView(
		actionId: string,
		run: (handlers: CanvasShortcutHandlers) => void,
	): void {
		if (isShortcutDisabled(actionId)) return;
		const hotkey = effectiveHotkey(actionId);
		if (!hotkey) return;
		handles.push(
			manager.register(
				hotkey,
				() => {
					const handlers = get();
					run(handlers);
				},
				{ target: root, ignoreInputs: true },
			),
		);
	}

	bindView('canvas.fit', (h) => h.fit());
	bindView('canvas.relayout', (h) => h.relayout());
	bindView('canvas.zoomIn', (h) => h.zoomIn());
	bindView('canvas.zoomOut', (h) => h.zoomOut());
	bindView('canvas.focusSelected', (h) => h.focusSelected());
	bindView('canvas.nextNode', (h) => h.nextNode());
	bindView('canvas.prevNode', (h) => h.prevNode());
	bindView('canvas.toggleLogs', (h) => h.toggleLogs());
	bindView('canvas.toggleFilters', (h) => h.toggleFilters());

	// Write operations are gated by edit mode.
	bindWrite('canvas.toggleEdit', (h) => h.toggleEdit());
	bindWrite('canvas.saveDraft', (h) => h.save());
	bindWrite('canvas.undo', (h) => h.undo());
	bindWrite('canvas.redo', (h) => h.redo());
	bindWrite('canvas.selectAll', (h) => h.selectAll());
	bindWrite('canvas.deleteSelection', (h) => h.deleteSelected());

	// Escape exits edit mode even while typing in a field.
	if (!isShortcutDisabled('canvas.exitEdit')) {
		handles.push(
			manager.register(
				effectiveHotkey('canvas.exitEdit') || 'Escape',
				() => {
					const handlers = get();
					if (handlers.canWrite()) handlers.toggleEdit();
				},
				{ target: root, ignoreInputs: false },
			),
		);
	}

	return () => {
		for (const handle of handles) handle.unregister();
		handles.length = 0;
	};
}
