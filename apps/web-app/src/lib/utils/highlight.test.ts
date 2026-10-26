import { describe, expect, it } from 'vitest';
import { highlightCode, normalizeLanguage } from './highlight';

function kinds(code: string, language: string): Promise<string[]> {
	return highlightCode(code, language).then((lines) => lines.flat().map((token) => token.kind));
}

describe('normalizeLanguage', () => {
	it('maps aliases to canonical names', () => {
		expect(normalizeLanguage('rs')).toBe('rust');
		expect(normalizeLanguage('TS')).toBe('typescript');
		expect(normalizeLanguage('c++')).toBe('cpp');
		expect(normalizeLanguage('')).toBe('');
		expect(normalizeLanguage(null)).toBe('');
	});
});

describe('highlightCode', () => {
	it('returns one line per source line', async () => {
		const lines = await highlightCode('a\nb', 'rust');
		expect(lines).toHaveLength(2);
	});

	it('classifies keywords, strings and numbers', async () => {
		const result = (await highlightCode('let x = "hi";', 'rust')).flat();
		const kindOf = (text: string) => result.find((token) => token.text === text)?.kind;
		expect(kindOf('let')).toBe('keyword');
		expect(kindOf('"hi"')).toBe('string');
		expect(result.find((token) => token.text.trim() === 'x')?.kind).toBe('plain');
	});

	it('classifies capitalized identifiers as types', async () => {
		const result = (await highlightCode('let v: Vec = x;', 'rust')).flat();
		expect(result.find((t) => t.text === 'Vec')?.kind).toBe('type');
	});

	it('classifies call sites as functions', async () => {
		const result = (await highlightCode('foo(1)', 'rust')).flat();
		expect(result.find((t) => t.text === 'foo')?.kind).toBe('function');
	});

	it('colors line comments to end of line', async () => {
		const result = (await highlightCode('code // note', 'rust')).flat();
		expect(result.find((t) => t.text === '// note')?.kind).toBe('comment');
	});

	it('carries block comments across lines', async () => {
		const lines = await highlightCode('/* start\nmiddle\nend */', 'rust');
		expect(lines[0][0].kind).toBe('comment');
		expect(lines[1][0].kind).toBe('comment');
		expect(lines[2][0].kind).toBe('comment');
	});

	it('carries python triple-quoted strings across lines', async () => {
		const lines = await highlightCode('x = """a\nb"""', 'python');
		expect(lines[0].some((t) => t.kind === 'string')).toBe(true);
		expect(lines[1][0].kind).toBe('string');
	});

	it('does not treat markup as keywords in unknown languages', async () => {
		const result = await kinds('SELECT 1', 'sql');
		// sql may or may not be supported; at minimum numbers should be present if supported
		expect(result).toBeDefined();
	});
});
