import { describe, expect, it } from 'vitest';
import {
	letterClassFor,
	letterForGitLevel,
	resolveDirDecoration,
	resolveFileDecoration,
	textClassFor,
} from './file-decorations';

describe('letterForGitLevel', () => {
	it('maps every git level to one stable letter', () => {
		expect(letterForGitLevel('added')).toBe('A');
		expect(letterForGitLevel('modified')).toBe('M');
		expect(letterForGitLevel('deleted')).toBe('D');
		expect(letterForGitLevel('renamed')).toBe('R');
		expect(letterForGitLevel('conflicted')).toBe('C');
		expect(letterForGitLevel('untracked')).toBe('U');
	});
});

describe('resolveFileDecoration', () => {
	it('returns null for clean files', () => {
		expect(resolveFileDecoration({ git: null, errors: 0, warnings: 0 })).toBeNull();
	});

	it('keeps the git letter for changed files', () => {
		const decoration = resolveFileDecoration({ git: 'modified', errors: 0, warnings: 0 });
		expect(decoration?.letter).toBe('M');
		expect(decoration?.textClass).toContain('text-warning');
	});

	it('lets errors outrank git colors', () => {
		const decoration = resolveFileDecoration({ git: 'modified', errors: 2, warnings: 1 });
		expect(decoration?.letter).toBe('M');
		expect(decoration?.letterClass).toBe('text-destructive');
		expect(decoration?.textClass).toBe('text-destructive');
		expect(decoration?.hasErrors).toBe(true);
	});

	it('colors problem-only files without a git letter', () => {
		const decoration = resolveFileDecoration({ git: null, errors: 0, warnings: 3 });
		expect(decoration?.letter).toBeNull();
		expect(decoration?.letterClass).toBe('text-warning');
		expect(decoration?.textClass).toBe('text-warning');
		expect(decoration?.hasWarnings).toBe(true);
	});
});

describe('resolveDirDecoration', () => {
	it('shares the file merge for aggregated directory inputs', () => {
		expect(resolveDirDecoration({ git: 'added', errors: 1, warnings: 0 })?.letter).toBe('A');
		expect(resolveDirDecoration({ git: null, errors: 0, warnings: 0 })).toBeNull();
	});
});

describe('tone helpers', () => {
	it('falls back to muted for unknown states', () => {
		expect(letterClassFor({ git: null, errors: 0, warnings: 0 })).toBe('text-muted-foreground');
		expect(textClassFor({ git: null, errors: 0, warnings: 0 })).toBe('');
	});
});
