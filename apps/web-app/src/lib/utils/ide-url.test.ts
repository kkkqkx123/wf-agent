import { describe, expect, it } from 'vitest';
import { buildIdeUrl, clampIdeLocation, parseIdeQuery } from './ide-url';

describe('buildIdeUrl', () => {
	it('builds a path-only address', () => {
		expect(buildIdeUrl('src/a.ts')).toContain('path=src%2Fa.ts');
	});

	it('carries 1-based line and column', () => {
		const url = buildIdeUrl('src/a.ts', 10, 4);
		expect(url).toContain('line=10');
		expect(url).toContain('col=4');
	});

	it('drops non-positive positions', () => {
		const url = buildIdeUrl('src/a.ts', 0, -3);
		expect(url).not.toContain('line=');
		expect(url).not.toContain('col=');
	});
});

describe('parseIdeQuery', () => {
	it('parses path with line and column', () => {
		const parsed = parseIdeQuery(new URLSearchParams('path=src%2Fa.ts&line=10&col=4'));
		expect(parsed).toEqual({ path: 'src/a.ts', line: 10, col: 4 });
	});

	it('returns null without a path', () => {
		expect(parseIdeQuery(new URLSearchParams('line=10'))).toBeNull();
	});
});

describe('clampIdeLocation', () => {
	it('clamps a line past the end of file', () => {
		expect(clampIdeLocation(999, null, 12)).toEqual({ line: 12, col: null });
	});

	it('keeps a missing line empty', () => {
		expect(clampIdeLocation(null, 4, 12)).toEqual({ line: null, col: null });
	});
});
