/**
 * Declarative catalog of application shortcuts. Every shortcut lives here
 * once: the help overlay, the settings editor and the active key handlers
 * all derive from these rows, so remapping a shortcut never desyncs the
 * displays from the behavior.
 */

export type ShortcutScope = 'global' | 'canvas';

export interface ShortcutAction {
	/** Stable identifier used by overrides storage. */
	id: string;
	/** Human label shown in settings and help. */
	label: string;
	/** Default binding in TanStack hotkey notation ('Mod' = Cmd on mac). */
	defaultKeys: string;
	/** Where the shortcut is active. */
	scope: ShortcutScope;
}

export const SHORTCUT_ACTIONS: ShortcutAction[] = [
	// Global (app shell / transcript).
	{ id: 'app.help', label: 'Toggle help', defaultKeys: 'F1', scope: 'global' },
	{ id: 'app.commandPalette', label: 'Command palette', defaultKeys: 'Mod+K', scope: 'global' },
	{ id: 'transcript.toTop', label: 'Scroll transcript to top', defaultKeys: 'Mod+Home', scope: 'global' },
	{ id: 'transcript.toBottom', label: 'Scroll transcript to bottom', defaultKeys: 'Mod+End', scope: 'global' },
	{ id: 'transcript.anchorNext', label: 'Jump to next anchor', defaultKeys: 'Mod+]', scope: 'global' },
	{ id: 'transcript.anchorPrev', label: 'Jump to previous anchor', defaultKeys: 'Mod+[', scope: 'global' },
	// Graph canvas.
	{ id: 'canvas.fit', label: 'Fit graph to view', defaultKeys: 'f', scope: 'canvas' },
	{ id: 'canvas.relayout', label: 'Re-run layout', defaultKeys: 'r', scope: 'canvas' },
	{ id: 'canvas.zoomIn', label: 'Zoom in', defaultKeys: '+', scope: 'canvas' },
	{ id: 'canvas.zoomOut', label: 'Zoom out', defaultKeys: '-', scope: 'canvas' },
	{ id: 'canvas.focusSelected', label: 'Focus selected node', defaultKeys: 'c', scope: 'canvas' },
	{ id: 'canvas.toggleEdit', label: 'Enter or exit edit mode', defaultKeys: 'e', scope: 'canvas' },
	{ id: 'canvas.exitEdit', label: 'Exit edit mode', defaultKeys: 'Escape', scope: 'canvas' },
	{ id: 'canvas.saveDraft', label: 'Save draft', defaultKeys: 'Mod+s', scope: 'canvas' },
	{ id: 'canvas.undo', label: 'Undo', defaultKeys: 'Mod+z', scope: 'canvas' },
	{ id: 'canvas.redo', label: 'Redo', defaultKeys: 'Shift+Mod+z', scope: 'canvas' },
	{ id: 'canvas.selectAll', label: 'Select all nodes', defaultKeys: 'Mod+a', scope: 'canvas' },
	{ id: 'canvas.deleteSelection', label: 'Delete selection', defaultKeys: 'Delete', scope: 'canvas' }
];

export function shortcutActionById(id: string): ShortcutAction | undefined {
	return SHORTCUT_ACTIONS.find((action) => action.id === id);
}

/** Help rows for one scope, derived from the catalog. */
export function shortcutHelpRows(
	scope: ShortcutScope
): Array<{ keys: string; action: string }> {
	return SHORTCUT_ACTIONS.filter((action) => action.scope === scope).map((action) => ({
		keys: action.defaultKeys,
		action: action.label
	}));
}
