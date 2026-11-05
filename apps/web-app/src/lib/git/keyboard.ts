import type { GitWorkbench, PanelId } from './workbench.svelte';
import { PANEL_ORDER } from './workbench.svelte';
import { parseToken } from './parse';

function editableTarget(target: EventTarget | null): boolean {
	const element = target as HTMLElement | null;
	if (!element || typeof element.closest !== 'function') return false;
	return element.closest('input, textarea, select, [contenteditable="true"], .xterm') !== null;
}

export interface PageKeyActions {
	/** Focus the rendered board row for a panel selection. */
	focusBoardRow(panel: PanelId, id: string): void;
	/** Focus the first board row of a panel, falling back to its filter input. */
	focusFirstBoardRow(panel: PanelId): void;
}

/**
 * Window-level keyboard handler for the git page. Selection and panel state
 * live in the workbench; this module only dispatches keys and manages focus.
 */
export function createPageKeyHandler(
	workbench: GitWorkbench,
	actions: PageKeyActions,
): (event: KeyboardEvent) => Promise<void> {
	return async (event: KeyboardEvent): Promise<void> => {
		if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key === '/') {
			event.preventDefault();
			workbench.focusKeyInput();
			return;
		}
		if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
		if (workbench.dialogOpen || workbench.helpOpen) return;
		if (editableTarget(event.target)) return;
		const target = event.target as HTMLElement | null;
		const inBoard = target?.closest?.('#git-board') != null;
		const panel = workbench.panel;

		if (event.key === 'Tab' && inBoard && PANEL_ORDER.includes(panel)) {
			event.preventDefault();
			actions.focusFirstBoardRow(workbench.cyclePanel(event.shiftKey ? -1 : 1));
			return;
		}
		if (
			(event.key === 'j' ||
				event.key === 'k' ||
				event.key === 'ArrowDown' ||
				event.key === 'ArrowUp') &&
			inBoard &&
			PANEL_ORDER.includes(panel)
		) {
			event.preventDefault();
			const delta = event.key === 'j' || event.key === 'ArrowDown' ? 1 : -1;
			const id = await workbench.stepSelection(delta);
			if (id) actions.focusBoardRow(panel, id);
			return;
		}
		if (event.key === '?') {
			workbench.helpOpen = true;
			return;
		}
		if (event.key === '/') {
			event.preventDefault();
			workbench.focusFilter();
			return;
		}
		if (event.key === 'Enter') {
			const element = event.target as HTMLElement | null;
			if (element?.closest?.('button, input, select, textarea, [contenteditable="true"], .xterm'))
				return;
			if (panel === 'files' || panel === 'conflicts') {
				event.preventDefault();
				workbench.setViewMode('staging');
				return;
			}
			if (panel === 'commits') {
				event.preventDefault();
				workbench.setViewMode(workbench.viewMode === 'commitFiles' ? 'diff' : 'commitFiles');
				return;
			}
			return;
		}
		if (event.key === 'Escape') {
			if ((panel === 'files' || panel === 'conflicts') && workbench.viewMode === 'staging') {
				workbench.setViewMode('diff');
				workbench.layoutFocus = 'board';
			} else if (panel === 'commits' && workbench.viewMode === 'commitFiles') {
				workbench.setViewMode('diff');
				workbench.layoutFocus = 'board';
			} else if (workbench.layoutFocus === 'main' || workbench.layoutFocus === 'board') {
				workbench.layoutFocus = 'none';
			} else if (inBoard && workbench.layoutFocus === 'none') {
				workbench.layoutFocus = 'board';
			} else if (!inBoard) {
				workbench.layoutFocus = 'none';
			}
			(event.target as HTMLElement | null)?.blur?.();
			const fallback =
				document.querySelector<HTMLButtonElement>('#git-board [data-board-row]') ??
				document.querySelector<HTMLButtonElement>('#git-panel button');
			fallback?.focus();
			return;
		}
		if (event.key === '0') {
			event.preventDefault();
			workbench.layoutFocus = 'main';
			document.querySelector<HTMLButtonElement>('#git-main button, #git-main input')?.focus();
			return;
		}
		// Printable keys run the bound panel action. Staging rows own their own
		// Space/Enter handling and stop propagation, so only unhandled keys
		// reach the token parser here.
		if (event.key.length === 1 || event.key === ' ') {
			if (event.key === ' ') {
				// A focused board row is a plain selection button: native Space
				// would only re-select it, so run the panel Space binding
				// (stage.toggle / worktree.switch) instead. Other buttons keep
				// native activation.
				if (target?.closest?.('[data-board-row]')) {
					event.preventDefault();
					const result = parseToken(' ', panel);
					if (result.kind === 'action' && result.action) {
						await workbench.runActionId(result.action);
					}
					return;
				}
				if (target?.closest?.('button')) return;
			}
			const result = parseToken(event.key, panel);
			if (result.kind === 'action' && result.action) {
				event.preventDefault();
				await workbench.runActionId(result.action);
			}
		}
	};
}
