/**
 * Declarative catalog of application shortcuts. Every shortcut lives here
 * once: the help overlay, the settings editor and the active key handlers
 * all derive from these rows, so remapping a shortcut never desyncs the
 * displays from the behavior.
 */

export type ShortcutScope = 'global' | 'canvas' | 'ide';

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
	{
		id: 'app.commandPalette',
		label: 'Command palette',
		defaultKeys: 'Mod+K',
		scope: 'global',
	},
	{
		id: 'transcript.toTop',
		label: 'Scroll transcript to top',
		defaultKeys: 'Mod+Home',
		scope: 'global',
	},
	{
		id: 'transcript.toBottom',
		label: 'Scroll transcript to bottom',
		defaultKeys: 'Mod+End',
		scope: 'global',
	},
	{
		id: 'transcript.anchorNext',
		label: 'Jump to next anchor',
		defaultKeys: 'Mod+]',
		scope: 'global',
	},
	{
		id: 'transcript.anchorPrev',
		label: 'Jump to previous anchor',
		defaultKeys: 'Mod+[',
		scope: 'global',
	},
	{
		id: 'transcript.detailLevel',
		label: 'Cycle tool output detail',
		defaultKeys: 'Mod+d',
		scope: 'global',
	},
	// Graph canvas.
	{
		id: 'canvas.fit',
		label: 'Fit graph to view',
		defaultKeys: 'f',
		scope: 'canvas',
	},
	{
		id: 'canvas.relayout',
		label: 'Re-run layout',
		defaultKeys: 'r',
		scope: 'canvas',
	},
	{ id: 'canvas.zoomIn', label: 'Zoom in', defaultKeys: '+', scope: 'canvas' },
	{
		id: 'canvas.zoomOut',
		label: 'Zoom out',
		defaultKeys: '-',
		scope: 'canvas',
	},
	{
		id: 'canvas.focusSelected',
		label: 'Focus selected node',
		defaultKeys: 'c',
		scope: 'canvas',
	},
	{
		id: 'canvas.toggleEdit',
		label: 'Enter or exit edit mode',
		defaultKeys: 'e',
		scope: 'canvas',
	},
	{
		id: 'canvas.exitEdit',
		label: 'Exit edit mode',
		defaultKeys: 'Escape',
		scope: 'canvas',
	},
	{
		id: 'canvas.saveDraft',
		label: 'Save draft',
		defaultKeys: 'Mod+s',
		scope: 'canvas',
	},
	{ id: 'canvas.undo', label: 'Undo', defaultKeys: 'Mod+z', scope: 'canvas' },
	{
		id: 'canvas.redo',
		label: 'Redo',
		defaultKeys: 'Shift+Mod+z',
		scope: 'canvas',
	},
	{
		id: 'canvas.selectAll',
		label: 'Select all nodes',
		defaultKeys: 'Mod+a',
		scope: 'canvas',
	},
	{
		id: 'canvas.deleteSelection',
		label: 'Delete selection',
		defaultKeys: 'Delete',
		scope: 'canvas',
	},
	{
		id: 'canvas.nextNode',
		label: 'Select next node',
		defaultKeys: 'n',
		scope: 'canvas',
	},
	{
		id: 'canvas.prevNode',
		label: 'Select previous node',
		defaultKeys: 'p',
		scope: 'canvas',
	},
	{
		id: 'canvas.toggleLogs',
		label: 'Toggle recent runs panel',
		defaultKeys: 'l',
		scope: 'canvas',
	},
	{
		id: 'canvas.toggleFilters',
		label: 'Toggle graph filters',
		defaultKeys: '/',
		scope: 'canvas',
	},
	// IDE (lightweight workspace editor).
	{
		id: 'ide.saveFile',
		label: 'Save current file',
		defaultKeys: 'Mod+s',
		scope: 'ide',
	},
	{
		id: 'ide.saveAll',
		label: 'Save all files',
		defaultKeys: 'Shift+Mod+s',
		scope: 'ide',
	},
	{
		id: 'ide.closeTab',
		label: 'Close current tab',
		defaultKeys: 'Mod+w',
		scope: 'ide',
	},
	{
		id: 'ide.toggleSidebar',
		label: 'Toggle file tree',
		defaultKeys: 'Mod+b',
		scope: 'ide',
	},
	{
		id: 'ide.toggleTerminal',
		label: 'Toggle terminal panel',
		defaultKeys: 'Mod+j',
		scope: 'ide',
	},
	{
		id: 'ide.quickOpen',
		label: 'Quick open file',
		defaultKeys: 'Mod+p',
		scope: 'ide',
	},
	{
		id: 'ide.contentSearch',
		label: 'Search file contents',
		defaultKeys: 'Shift+Mod+f',
		scope: 'ide',
	},
	{
		id: 'ide.revealInTree',
		label: 'Reveal current file in tree',
		defaultKeys: 'Shift+Mod+e',
		scope: 'ide',
	},
	{
		id: 'ide.copyPath',
		label: 'Copy current file path',
		defaultKeys: 'Shift+Mod+c',
		scope: 'ide',
	},
	{
		id: 'ide.cut',
		label: 'Cut selection in file tree',
		defaultKeys: 'Mod+x',
		scope: 'ide',
	},
	{
		id: 'ide.copy',
		label: 'Copy selection in file tree',
		defaultKeys: 'Mod+c',
		scope: 'ide',
	},
	{
		id: 'ide.paste',
		label: 'Paste clipboard into folder',
		defaultKeys: 'Mod+v',
		scope: 'ide',
	},
	{
		id: 'ide.selectAll',
		label: 'Select all in file tree',
		defaultKeys: 'Mod+a',
		scope: 'ide',
	},
	{
		id: 'ide.clearSelection',
		label: 'Clear file tree selection',
		defaultKeys: 'Escape',
		scope: 'ide',
	},
	{
		id: 'ide.upload',
		label: 'Upload files to folder',
		defaultKeys: 'Shift+Mod+u',
		scope: 'ide',
	},
	{
		id: 'ide.download',
		label: 'Download selected file',
		defaultKeys: 'Shift+Mod+d',
		scope: 'ide',
	},
	{
		id: 'ide.toggleHistory',
		label: 'Toggle history sidebar',
		defaultKeys: 'Shift+Mod+h',
		scope: 'ide',
	},
	{
		id: 'ide.nextChange',
		label: 'Jump to next change',
		defaultKeys: 'Alt+ArrowDown',
		scope: 'ide',
	},
	{
		id: 'ide.prevChange',
		label: 'Jump to previous change',
		defaultKeys: 'Alt+ArrowUp',
		scope: 'ide',
	},
	{
		id: 'ide.viewDiff',
		label: 'View diff of selected file',
		defaultKeys: 'Shift+Mod+g',
		scope: 'ide',
	},
];

export function shortcutActionById(id: string): ShortcutAction | undefined {
	return SHORTCUT_ACTIONS.find((action) => action.id === id);
}

/** Help rows for one scope, derived from the catalog. */
export function shortcutHelpRows(scope: ShortcutScope): Array<{ keys: string; action: string }> {
	return SHORTCUT_ACTIONS.filter((action) => action.scope === scope).map((action) => ({
		keys: action.defaultKeys,
		action: action.label,
	}));
}
