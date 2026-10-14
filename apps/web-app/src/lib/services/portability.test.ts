import { describe, expect, it } from 'vitest';
import {
	BACKUP_VERSION,
	describeBackup,
	parseBackupDocument,
	serializeBackupDocument,
	validateBackupDocument,
	type SettingsBackup,
} from '$lib/services/portability';

function validBackup(): SettingsBackup {
	return {
		version: 1,
		preferences: {
			theme: 'dark',
			density: 'default',
			sidebarCollapsed: false,
			sidebarWidth: 240,
			inspectorWidth: 360,
			inspectorPinned: false,
			minimapMode: 'auto',
			toolOutputPreviews: true,
			toolOutputDetail: 'balanced',
		},
		shortcuts: { 'app.commandPalette': 'Mod+K' },
		behavior: {
			pageSize: 50,
			autoRefresh: true,
			streamFollow: true,
			reduceMotion: false,
		},
	};
}

describe('settings backup roundtrip', () => {
	it('serializes and parses back to the same document', () => {
		const backup = validBackup();
		const parsed = parseBackupDocument(serializeBackupDocument(backup));
		expect(parsed).toEqual(backup);
		expect(parsed.version).toBe(BACKUP_VERSION);
	});

	it('summarizes the replaced sections', () => {
		expect(describeBackup(validBackup())).toContain('Appearance');
	});
});

describe('settings backup rejection', () => {
	it('rejects unknown versions', () => {
		const backup = { ...validBackup(), version: 2 };
		expect(() => validateBackupDocument(backup)).toThrow(/version/i);
	});

	it('rejects unknown top-level keys', () => {
		const backup = { ...validBackup(), extra: 1 };
		expect(() => validateBackupDocument(backup)).toThrow(/Unknown backup key/);
	});

	it('rejects unknown preference keys', () => {
		const backup = validBackup();
		const raw = {
			...backup,
			preferences: { ...backup.preferences, language: 'en' },
		};
		expect(() => validateBackupDocument(raw)).toThrow(/preferences/);
	});

	it('rejects mistyped behavior values', () => {
		const backup = validBackup();
		const raw = {
			...backup,
			behavior: { ...backup.behavior, pageSize: 'fifty' },
		};
		expect(() => validateBackupDocument(raw)).toThrow(/pageSize/);
	});

	it('rejects unknown shortcut actions', () => {
		const backup = validBackup();
		const raw = { ...backup, shortcuts: { 'nope.action': 'Ctrl+X' } };
		expect(() => validateBackupDocument(raw)).toThrow(/shortcut/i);
	});

	it('rejects non-JSON text', () => {
		expect(() => parseBackupDocument('not json {')).toThrow(/JSON/);
	});

	it('falls back nothing: corrupt input throws instead of repairing', () => {
		expect(() => validateBackupDocument(null)).toThrow();
		expect(() => validateBackupDocument({})).toThrow();
	});
});
