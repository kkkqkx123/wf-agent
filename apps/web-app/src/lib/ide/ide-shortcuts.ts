import { getHotkeyManager } from '@tanstack/hotkeys';
import { effectiveHotkey, isShortcutDisabled, resolvedShortcutRows } from '#lib/shortcuts/registry';

/**
 * Shared vocabulary for IDE shortcuts. The global help overlay renders
 * these rows, so renaming a shortcut only happens in the shortcut catalog.
 */
export const IDE_SHORTCUT_HELP: Array<{ keys: string; action: string }> = resolvedShortcutRows(
	'ide',
)
	.filter((row) => !row.disabled)
	.map((row) => ({ keys: row.keys, action: row.action.label }));

export interface IdeShortcutHandlers {
	saveFile(): void;
	saveAll(): void;
	closeTab(): void;
	toggleSidebar(): void;
	toggleTerminal(): void;
	quickOpen(): void;
	contentSearch(): void;
	revealInTree(): void;
	copyPath(): void;
	cut(): void;
	copy(): void;
	paste(): void;
	selectAll(): void;
	clearSelection(): void;
	upload(): void;
	download(): void;
	toggleHistory(): void;
	nextChange(): void;
	prevChange(): void;
	viewDiff(): void;
}

/**
 * Register IDE shortcuts scoped to focus inside `root` (bindings attach to
 * the root element, so events bubble only from within it). Save and close
 * use `ignoreInputs: false` so they keep working while typing in the inline
 * rename inputs; view toggles ignore plain inputs to preserve native
 * behavior. Returns a dispose function.
 */
export function registerIdeShortcuts(
	root: HTMLElement,
	get: () => IdeShortcutHandlers,
): () => void {
	const manager = getHotkeyManager();
	const handles: Array<{ unregister(): void }> = [];

	function bind(actionId: string, run: (handlers: IdeShortcutHandlers) => void): void {
		if (isShortcutDisabled(actionId)) return;
		const hotkey = effectiveHotkey(actionId);
		if (!hotkey) return;
		try {
			handles.push(
				manager.register(
					hotkey,
					(event) => {
						event.preventDefault();
						run(get());
					},
					{ target: root, ignoreInputs: false },
				),
			);
		} catch {
			// An unparsable binding must not break the remaining shortcuts.
		}
	}

	bind('ide.saveFile', (h) => h.saveFile());
	bind('ide.saveAll', (h) => h.saveAll());
	bind('ide.closeTab', (h) => h.closeTab());
	bind('ide.toggleSidebar', (h) => h.toggleSidebar());
	bind('ide.toggleTerminal', (h) => h.toggleTerminal());
	bind('ide.quickOpen', (h) => h.quickOpen());
	bind('ide.contentSearch', (h) => h.contentSearch());
	bind('ide.revealInTree', (h) => h.revealInTree());
	bind('ide.copyPath', (h) => h.copyPath());
	bind('ide.cut', (h) => h.cut());
	bind('ide.copy', (h) => h.copy());
	bind('ide.paste', (h) => h.paste());
	bind('ide.selectAll', (h) => h.selectAll());
	bind('ide.clearSelection', (h) => h.clearSelection());
	bind('ide.upload', (h) => h.upload());
	bind('ide.download', (h) => h.download());
	bind('ide.toggleHistory', (h) => h.toggleHistory());
	bind('ide.nextChange', (h) => h.nextChange());
	bind('ide.prevChange', (h) => h.prevChange());
	bind('ide.viewDiff', (h) => h.viewDiff());

	return () => {
		for (const handle of handles) handle.unregister();
		handles.length = 0;
	};
}
