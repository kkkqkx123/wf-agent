import { SHORTCUT_ACTIONS } from '#lib/shortcuts/actions';

/**
 * Portable settings backup: local appearance track, shortcut overrides and
 * server behavior document in one versioned file. Version stays at one;
 * unknown versions, unknown keys and type errors are rejected, never
 * repaired. Session drafts, lock identity and secrets never enter a backup.
 */

export const BACKUP_VERSION = 1;

export type BackupTheme = 'light' | 'dark' | 'system';
export type BackupDensity = 'compact' | 'default' | 'comfortable';
export type BackupMinimap = 'auto' | 'on' | 'off';
export type BackupToolDetail = 'compact' | 'balanced' | 'verbose';

export interface LocalPreferencesBackup {
	theme: BackupTheme;
	density: BackupDensity;
	sidebarCollapsed: boolean;
	sidebarWidth: number;
	inspectorWidth: number;
	inspectorPinned: boolean;
	minimapMode: BackupMinimap;
	toolOutputPreviews: boolean;
	toolOutputDetail: BackupToolDetail;
}

export interface BehaviorBackup {
	pageSize: number;
	streamFollow: boolean;
	reduceMotion: boolean;
}

export type ShortcutBackup = Record<string, string | null>;

export interface SettingsBackup {
	version: number;
	preferences: LocalPreferencesBackup;
	shortcuts: ShortcutBackup;
	behavior: BehaviorBackup;
}

const KNOWN_ACTION_IDS = new Set(SHORTCUT_ACTIONS.map((action) => action.id));

const PREFERENCE_KEYS: ReadonlyArray<keyof LocalPreferencesBackup> = [
	'theme',
	'density',
	'sidebarCollapsed',
	'sidebarWidth',
	'inspectorWidth',
	'inspectorPinned',
	'minimapMode',
	'toolOutputPreviews',
	'toolOutputDetail',
];

const BEHAVIOR_KEYS: ReadonlyArray<keyof BehaviorBackup> = [
	'pageSize',
	'streamFollow',
	'reduceMotion',
];

const PAGE_SIZE_OPTIONS = [25, 50, 100];

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function reject(message: string): never {
	throw new Error(message);
}

function checkNoUnknownKeys(
	value: Record<string, unknown>,
	known: ReadonlyArray<string>,
	section: string,
): void {
	for (const key of Object.keys(value)) {
		if (!known.includes(key)) reject(`Unknown ${section} key: ${key}`);
	}
}

function checkBoolean(value: unknown, field: string): boolean {
	if (typeof value !== 'boolean') reject(`Invalid ${field}: expected boolean`);
	return value as boolean;
}

function validatePreferences(raw: unknown): LocalPreferencesBackup {
	if (!isObject(raw)) reject('Invalid preferences: expected object');
	checkNoUnknownKeys(raw, PREFERENCE_KEYS, 'preferences');
	const { theme, density, minimapMode, toolOutputDetail } = raw;
	if (theme !== 'light' && theme !== 'dark' && theme !== 'system') {
		reject('Invalid preferences.theme');
	}
	if (
		density !== 'compact' &&
		density !== 'default' &&
		density !== 'comfortable'
	) {
		reject('Invalid preferences.density');
	}
	if (minimapMode !== 'auto' && minimapMode !== 'on' && minimapMode !== 'off') {
		reject('Invalid preferences.minimapMode');
	}
	if (
		toolOutputDetail !== 'compact' &&
		toolOutputDetail !== 'balanced' &&
		toolOutputDetail !== 'verbose'
	) {
		reject('Invalid preferences.toolOutputDetail');
	}
	const sidebarWidth = raw.sidebarWidth;
	const inspectorWidth = raw.inspectorWidth;
	if (
		typeof sidebarWidth !== 'number' ||
		!Number.isFinite(sidebarWidth) ||
		sidebarWidth < 200 ||
		sidebarWidth > 360
	) {
		reject('Invalid preferences.sidebarWidth');
	}
	if (
		typeof inspectorWidth !== 'number' ||
		!Number.isFinite(inspectorWidth) ||
		inspectorWidth < 240 ||
		inspectorWidth > 640
	) {
		reject('Invalid preferences.inspectorWidth');
	}
	return {
		theme: theme as BackupTheme,
		density: density as BackupDensity,
		sidebarCollapsed: checkBoolean(
			raw.sidebarCollapsed,
			'preferences.sidebarCollapsed',
		),
		sidebarWidth: sidebarWidth as number,
		inspectorWidth: inspectorWidth as number,
		inspectorPinned: checkBoolean(
			raw.inspectorPinned,
			'preferences.inspectorPinned',
		),
		minimapMode: minimapMode as BackupMinimap,
		toolOutputPreviews: checkBoolean(
			raw.toolOutputPreviews,
			'preferences.toolOutputPreviews',
		),
		toolOutputDetail: toolOutputDetail as BackupToolDetail,
	};
}

function validateShortcuts(raw: unknown): ShortcutBackup {
	if (!isObject(raw)) reject('Invalid shortcuts: expected object');
	const out: ShortcutBackup = {};
	for (const [id, value] of Object.entries(raw)) {
		if (!KNOWN_ACTION_IDS.has(id)) reject(`Unknown shortcut action: ${id}`);
		if (value === null) {
			out[id] = null;
		} else if (
			typeof value === 'string' &&
			value.length > 0 &&
			value.length <= 64
		) {
			out[id] = value;
		} else {
			reject(`Invalid shortcut binding for ${id}`);
		}
	}
	return out;
}

function validateBehavior(raw: unknown): BehaviorBackup {
	if (!isObject(raw)) reject('Invalid behavior: expected object');
	checkNoUnknownKeys(raw, BEHAVIOR_KEYS, 'behavior');
	if (!PAGE_SIZE_OPTIONS.includes(raw.pageSize as number)) {
		reject('Invalid behavior.pageSize');
	}
	return {
		pageSize: raw.pageSize as number,
		streamFollow: checkBoolean(raw.streamFollow, 'behavior.streamFollow'),
		reduceMotion: checkBoolean(raw.reduceMotion, 'behavior.reduceMotion'),
	};
}

/** Validate a parsed backup document; rejects unknown versions and keys. */
export function validateBackupDocument(raw: unknown): SettingsBackup {
	if (!isObject(raw)) reject('Invalid backup: expected object');
	const keys = Object.keys(raw);
	for (const key of keys) {
		if (
			key !== 'version' &&
			key !== 'preferences' &&
			key !== 'shortcuts' &&
			key !== 'behavior'
		) {
			reject(`Unknown backup key: ${key}`);
		}
	}
	if (raw.version !== BACKUP_VERSION) {
		reject(`Unsupported backup version: ${String(raw.version)}`);
	}
	return {
		version: BACKUP_VERSION,
		preferences: validatePreferences(raw.preferences),
		shortcuts: validateShortcuts(raw.shortcuts),
		behavior: validateBehavior(raw.behavior),
	};
}

/** Parse backup file text into a validated document. */
export function parseBackupDocument(text: string): SettingsBackup {
	let raw: unknown;
	try {
		raw = JSON.parse(text) as unknown;
	} catch {
		reject('Invalid backup: not JSON');
	}
	return validateBackupDocument(raw);
}

/** Serialize a backup document for browser download. */
export function serializeBackupDocument(backup: SettingsBackup): string {
	return JSON.stringify(backup, null, 2);
}

/** Dated download filename so repeated exports stay distinguishable. */
export function backupFilename(now: Date = new Date()): string {
	const day = now.toISOString().slice(0, 10);
	return `wf-settings-${day}.json`;
}

/** One-line summary of what an import will replace. */
export function describeBackup(backup: SettingsBackup): string {
	const shortcutCount = Object.keys(backup.shortcuts).length;
	return `Appearance (${backup.preferences.theme}, ${backup.preferences.density}), ${shortcutCount} shortcut overrides, server behavior (page ${backup.behavior.pageSize})`;
}
