import { describe, expect, it } from 'vitest';
import { CLIENT_ONLY_ACTIONS, confirmTarget, parseToken } from './parse';

describe('parseToken', () => {
	it('prompts for the active context on empty input', () => {
		const result = parseToken('  ', 'files');
		expect(result.kind).toBe('empty');
		expect(result.preview).toContain('files');
	});

	it('lists context actions for the help token', () => {
		const result = parseToken('?', 'branches');
		expect(result.kind).toBe('help');
		expect(result.candidates).toContain('branch.checkout');
		expect(result.candidates).not.toContain('stage.toggle');
	});

	it('forwards colon-prefixed input as a terminal command', () => {
		const result = parseToken(':git status --short', 'files');
		expect(result).toEqual({ kind: 'command', preview: 'git status --short' });
	});

	it('resolves a single-char alias inside the context', () => {
		const result = parseToken('c', 'files');
		expect(result.kind).toBe('action');
		expect(result.action).toBe('commit');
		expect(result.danger).toBe('recoverable');
	});

	it('falls back to prefix search when the alias misses', () => {
		const result = parseToken('z', 'files');
		expect(result.kind).toBe('prefix');
		expect(result.candidates).toEqual([]);
	});

	it('prefers the context when a multi-char prefix is ambiguous', () => {
		const result = parseToken('stash', 'stash');
		expect(result.kind).toBe('prefix');
		expect(result.candidates?.[0]).toBe('stash.pop');
	});

	it('resolves a multi-char prefix that matches exactly one action', () => {
		const result = parseToken('branch.check', 'branches');
		expect(result.kind).toBe('action');
		expect(result.action).toBe('branch.checkout');
	});
});

describe('confirmTarget', () => {
	it('uses the path for discards', () => {
		expect(confirmTarget('discard', { path: 'a.rs' })).toBe('a.rs');
	});

	it('uses the branch name for branch deletion', () => {
		expect(confirmTarget('branch.delete', { branch: 'feature' })).toBe('feature');
	});

	it('joins remote and branch for pushes', () => {
		expect(confirmTarget('push', { remote: 'origin', branch: 'main' })).toBe('origin main');
		expect(confirmTarget('push', { remote: 'origin', branch: '' })).toBe('origin');
		expect(confirmTarget('push', {})).toBe('origin');
	});

	it('builds the stash reference for drops', () => {
		expect(confirmTarget('stash.drop', { index: '2' })).toBe('stash@{2}');
		expect(confirmTarget('stash.drop', {})).toBe('stash@{0}');
	});

	it('uses the fixed token for amend', () => {
		expect(confirmTarget('commit.amend', {})).toBe('amend');
	});
});

describe('CLIENT_ONLY_ACTIONS', () => {
	it('holds the view-only actions that never reach the server', () => {
		expect(CLIENT_ONLY_ACTIONS.has('focus')).toBe(true);
		expect(CLIENT_ONLY_ACTIONS.has('filter')).toBe(true);
		expect(CLIENT_ONLY_ACTIONS.has('commit')).toBe(false);
	});
});
