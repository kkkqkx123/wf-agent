/**
 * Persistence for user shortcut overrides. Overrides map an action id to
 * either a replacement hotkey string or `null` (disabled). Stored in
 * localStorage under a versioned key; a corrupt entry falls back to
 * defaults instead of breaking the app.
 */

const STORAGE_KEY = 'wf.shortcuts.overrides.v1';

export type ShortcutOverrides = Record<string, string | null>;

export function loadShortcutOverrides(): ShortcutOverrides {
	if (typeof window === 'undefined') return {};
	try {
		const raw = window.localStorage.getItem(STORAGE_KEY);
		if (!raw) return {};
		const parsed: unknown = JSON.parse(raw);
		if (typeof parsed !== 'object' || parsed === null) return {};
		const overrides: ShortcutOverrides = {};
		for (const [id, value] of Object.entries(
			parsed as Record<string, unknown>,
		)) {
			if (typeof value === 'string' && value.length > 0) {
				overrides[id] = value;
			} else if (value === null) {
				overrides[id] = null;
			}
		}
		return overrides;
	} catch {
		return {};
	}
}

export function saveShortcutOverrides(overrides: ShortcutOverrides): void {
	if (typeof window === 'undefined') return;
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
	} catch {
		// Storage may be unavailable (private mode / quota); overrides then
		// only live for the current page session.
	}
}
