import { describe, expect, it } from 'vitest';
import { iconForEntry, iconForFile } from './file-icons';

describe('iconForFile', () => {
	it('maps image extensions', () => {
		expect(iconForFile('assets/logo.png')).toBe('image');
		expect(iconForFile('assets/vector.SVG')).toBe('image');
	});

	it('maps shell and database extensions', () => {
		expect(iconForFile('scripts/deploy.sh')).toBe('terminal');
		expect(iconForFile('data/dump.sql')).toBe('database');
	});

	it('maps source extensions to code', () => {
		expect(iconForFile('src/main.ts')).toBe('code');
		expect(iconForFile('src/app.svelte')).toBe('code');
		expect(iconForFile('crates/foo/lib.rs')).toBe('code');
		expect(iconForFile('styles/app.CSS')).toBe('code');
	});

	it('falls back to a plain file for data, docs and unknown types', () => {
		expect(iconForFile('config/settings.json')).toBe('file');
		expect(iconForFile('docs/guide.md')).toBe('file');
		expect(iconForFile('README')).toBe('file');
		expect(iconForFile('.gitignore')).toBe('file');
		expect(iconForFile('archive.tar.gz')).toBe('file');
	});
});

describe('iconForEntry', () => {
	it('always uses the folder shape for directories', () => {
		expect(iconForEntry('src', true)).toBe('folder');
		expect(iconForEntry('src/main.ts', true)).toBe('folder');
	});

	it('delegates files to the extension mapping', () => {
		expect(iconForEntry('src/main.ts', false)).toBe('code');
		expect(iconForEntry('assets/logo.png', false)).toBe('image');
	});
});
