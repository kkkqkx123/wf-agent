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

	return () => {
		for (const handle of handles) handle.unregister();
		handles.length = 0;
	};
}
