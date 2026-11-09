import { describe, expect, it } from 'vitest';
import {
	buildTabLabel,
	directoryNamespace,
	mergeRecent,
	shortHostPrefix,
	shortWorkspaceName,
	shortenSegment,
	validateDirectoryFormat,
} from './workspace';

describe('validateDirectoryFormat', () => {
	it('rejects empty and relative spellings', () => {
		expect(validateDirectoryFormat('')).toBe('directory must not be empty');
		expect(validateDirectoryFormat('   ')).toBe('directory must not be empty');
		expect(validateDirectoryFormat('relative/path')).toBe('directory must be absolute');
	});

	it('accepts absolute paths', () => {
		expect(validateDirectoryFormat('/srv/wf/projects/acme')).toBeUndefined();
	});
});

describe('directoryNamespace', () => {
	it('stays empty without a directory so legacy keys keep working', () => {
		expect(directoryNamespace(undefined)).toBe('');
		expect(directoryNamespace('')).toBe('');
	});

	it('isolates distinct directories and ignores trailing slashes', () => {
		const a = directoryNamespace('/srv/a');
		const b = directoryNamespace('/srv/b');
		expect(a).toMatch(/^\.[0-9a-f]{8}$/);
		expect(b).toMatch(/^\.[0-9a-f]{8}$/);
		expect(a).not.toBe(b);
		expect(directoryNamespace('/srv/a/')).toBe(a);
	});
});

describe('shortWorkspaceName', () => {
	it('keeps the last two levels for context', () => {
		expect(shortWorkspaceName('/home/u/code/wf-agent')).toBe('code/wf-agent');
		expect(shortWorkspaceName('/srv/wf/projects/acme/')).toBe('projects/acme');
	});

	it('falls back to one level and the root', () => {
		expect(shortWorkspaceName('/repo')).toBe('repo');
		expect(shortWorkspaceName('/')).toBe('/');
		expect(shortWorkspaceName(undefined)).toBe('');
	});

	it('shortens overlong segments', () => {
		expect(shortenSegment('a'.repeat(40))).toBe(`${'a'.repeat(19)}…`);
		expect(shortWorkspaceName('/x/abcdefghijklmnopqrstuvwxyz0123456789')).toBe(
			'x/abcdefghijklmnopqrs…',
		);
	});
});

describe('shortHostPrefix', () => {
	it('takes the first four alphanumerics lowercased', () => {
		expect(shortHostPrefix('archlinux')).toBe('arch');
		expect(shortHostPrefix('Ubuntu-22.04')).toBe('ubun');
		expect(shortHostPrefix('WSL')).toBe('wsl');
		expect(shortHostPrefix(undefined)).toBe('');
		expect(shortHostPrefix('')).toBe('');
	});
});

describe('buildTabLabel', () => {
	it('prefers the explicit override', () => {
		expect(buildTabLabel('Acme API', '/srv/acme', 'archlinux')).toBe('Acme API');
	});

	it('derives the short name with an optional host prefix', () => {
		expect(buildTabLabel(undefined, '/home/u/code/wf-agent', undefined)).toBe('code/wf-agent');
		expect(buildTabLabel('', '/home/u/code/wf-agent', 'archlinux')).toBe('arch:code/wf-agent');
	});

	it('stays empty without a workspace or override', () => {
		expect(buildTabLabel(undefined, undefined, 'archlinux')).toBe('');
	});
});

describe('mergeRecent', () => {
	it('dedupes, orders newest first and caps the list', () => {
		const list = mergeRecent([], '/a', 1);
		const moved = mergeRecent([...list, { directory: '/b', lastUsed: 2 }], '/a', 3);
		expect(moved[0]).toEqual({ directory: '/a', lastUsed: 3 });
		expect(moved.map((entry) => entry.directory)).toContain('/b');
		const full = mergeRecent(
			Array.from({ length: 12 }, (_, i) => ({ directory: `/d${i}`, lastUsed: i })),
			'/new',
			99,
		);
		expect(full.length).toBeLessThanOrEqual(10);
		expect(full[0]?.directory).toBe('/new');
	});
});
