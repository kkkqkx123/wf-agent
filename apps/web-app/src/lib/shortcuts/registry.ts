/**
 * Shortcut registry: merges the action catalog with user overrides and
 * exposes conflict detection on top of the TanStack hotkeys manager.
 */

import { findHotkeyConflicts, validateHotkey, type RegisterableHotkey } from '@tanstack/hotkeys';
import { SHORTCUT_ACTIONS, shortcutActionById, type ShortcutAction } from './actions';
import { loadShortcutOverrides, saveShortcutOverrides, type ShortcutOverrides } from './settings';

export { SHORTCUT_ACTIONS, shortcutActionById, shortcutHelpRows } from './actions';
export type { ShortcutAction, ShortcutScope } from './actions';
export type { ShortcutOverrides } from './settings';

/** Effective hotkey for an action: user override when set, catalog default otherwise. */
export function effectiveHotkey(id: string): RegisterableHotkey {
	const override = loadShortcutOverrides()[id];
	const raw = typeof override === 'string' ? override : (shortcutActionById(id)?.defaultKeys ?? '');
	// Catalog entries and recorder output are validated hotkey strings; the
	// library's type is a template-literal refinement of `string`.
	return raw as RegisterableHotkey;
}

/** Whether an action is explicitly disabled by a user override. */
export function isShortcutDisabled(id: string): boolean {
	return loadShortcutOverrides()[id] === null;
}

/** All user overrides (for the settings editor). */
export function shortcutOverrides(): ShortcutOverrides {
	return loadShortcutOverrides();
}

/** Set or clear an override and persist it. `null` disables the action. */
export function setShortcutOverride(id: string, hotkey: string | null): void {
	const overrides = loadShortcutOverrides();
	if (hotkey === null) {
		overrides[id] = null;
	} else {
		delete overrides[id];
	}
	if (hotkey !== null) overrides[id] = hotkey;
	saveShortcutOverrides(overrides);
}

/** Remove a single override, restoring the catalog default. */
export function clearShortcutOverride(id: string): void {
	const overrides = loadShortcutOverrides();
	delete overrides[id];
	saveShortcutOverrides(overrides);
}

/** Validation result for a candidate hotkey string. */
export function validateShortcut(hotkey: string): {
	valid: boolean;
	error?: string;
} {
	try {
		const result = validateHotkey(hotkey);
		return result.valid
			? { valid: true }
			: { valid: false, error: result.errors.join('; ') || 'invalid hotkey' };
	} catch (error) {
		return {
			valid: false,
			error: error instanceof Error ? error.message : 'invalid hotkey',
		};
	}
}

/** Live registrations conflicting with the given hotkey string. */
export function findShortcutConflicts(hotkey: string) {
	return findHotkeyConflicts(hotkey as RegisterableHotkey);
}

/** Catalog rows enriched with their effective binding (help / settings display). */
export function resolvedShortcutRows(scope?: ShortcutAction['scope']): Array<{
	action: ShortcutAction;
	keys: string;
	disabled: boolean;
}> {
	const overrides = loadShortcutOverrides();
	return SHORTCUT_ACTIONS.filter((action) => !scope || action.scope === scope).map((action) => {
		const override = overrides[action.id];
		const keys = typeof override === 'string' ? override : action.defaultKeys;
		return { action, keys, disabled: override === null };
	});
}
