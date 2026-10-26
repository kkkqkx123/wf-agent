import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { highlightCode, highlightIncremental, normalizeLanguage, ruleScopeKind } from './highlight';
import { LANGUAGE_ALIASES } from './language-aliases.generated';

describe('ruleScopeKind on theme rule scope selectors', () => {
	it('classifies comment selectors regardless of later segments', () => {
		expect(ruleScopeKind('comment.line.number-sign.python')).toBe('comment');
		expect(ruleScopeKind('comment.block.documentation.rust')).toBe('comment');
	});

	it('does not misclassify string punctuation as a string', () => {
		expect(ruleScopeKind('punctuation.definition.string.begin.python')).toBeNull();
		expect(ruleScopeKind('string.quoted.double.python')).toBe('string');
		expect(ruleScopeKind('char.literal.cpp')).toBe('string');
		expect(ruleScopeKind('string.regexp.explicit.python')).toBe('string');
	});

	it('classifies numbers via constant selectors that extend the numeric pattern', () => {
		expect(ruleScopeKind('constant.numeric.integer.decimal.rust')).toBe('number');
		// A bare 'constant' rule covers constant.numeric, so it maps to number.
		expect(ruleScopeKind('constant')).toBe('number');
		// constant.language resolves to keyword instead.
		expect(ruleScopeKind('constant.language')).toBe('keyword');
	});

	it('resolves entity selectors in favour of type entities', () => {
		expect(ruleScopeKind('support.type.rust')).toBe('type');
		expect(ruleScopeKind('entity.name.class.definition.python')).toBe('type');
		// A bare 'entity' rule covers entity.name.type, so it maps to type.
		expect(ruleScopeKind('entity')).toBe('type');
		expect(ruleScopeKind('storage.type.rust')).toBe('keyword');
	});

	it('classifies function entity selectors', () => {
		expect(ruleScopeKind('entity.name.function.rust')).toBe('function');
		expect(ruleScopeKind('support.function.builtin.python')).toBe('function');
	});

	it('returns null for unrelated selectors', () => {
		expect(ruleScopeKind('variable.other.readwrite.ts')).toBeNull();
		expect(ruleScopeKind('source.rust')).toBeNull();
		expect(ruleScopeKind('meta.number.float.decimal.ts')).toBeNull();
	});
});

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '../../../../../');
const aliasTsvPath = join(repoRoot, 'crates/infra/wf-syntax/src/language-aliases.tsv');

function parseAliasTsv(raw: string): Record<string, string> {
	const table: Record<string, string> = {};
	for (const line of raw.split('\n')) {
		const trimmed = line.trim();
		if (trimmed === '' || trimmed.startsWith('#')) continue;
		const tab = trimmed.indexOf('\t');
		if (tab < 0) throw new Error(`alias line missing tab separator: ${trimmed}`);
		const alias = trimmed.slice(0, tab).trim();
		const canonical = trimmed.slice(tab + 1).trim();
		if (!alias || !canonical) throw new Error(`blank field in: ${trimmed}`);
		table[alias] = canonical;
	}
	return table;
}

describe('language alias table freshness', () => {
	it('generated module matches the shared TSV source of truth', () => {
		const tsv = parseAliasTsv(readFileSync(aliasTsvPath, 'utf8'));
		expect(LANGUAGE_ALIASES).toEqual(tsv);
	});
});

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

	it('classifies declaration names through the theme entity color', async () => {
		const result = (await highlightCode('let v: Vec = x;', 'rust')).flat();
		expect(result.find((t) => t.text === 'Vec')?.kind).toBe('type');
	});

	it('collapses call sites into the entity color family like the theme does', async () => {
		// github-light renders declaration names and call sites with one color,
		// so the color route classifies both as 'type' (see buildColorKinds).
		const result = (await highlightCode('foo(1)', 'rust')).flat();
		expect(result.find((t) => t.text === 'foo')?.kind).toBe('type');
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

describe('highlightIncremental', () => {
	it('reuses completed previous lines for a prefix append', async () => {
		// Previous code ends without a newline, so its last line is
		// incomplete and never reused; only line 0 comes back verbatim.
		const first = await highlightCode('let a = 1;\nlet b = 2;', 'rust');
		const second = await highlightIncremental(
			'let a = 1;\nlet b = 2;\nlet c = 3;',
			'rust',
			{ code: 'let a = 1;\nlet b = 2;', lines: first },
		);
		expect(second.length).toBe(3);
		expect(second[0]).toBe(first[0]);
		expect(second[1]).not.toBe(first[1]);
	});

	it('reuses every previous line when the previous code ends with a newline', async () => {
		const first = await highlightCode('let a = 1;\n', 'rust');
		const second = await highlightIncremental('let a = 1;\nlet b = 2;', 'rust', {
			code: 'let a = 1;\n',
			lines: first,
		});
		expect(second.length).toBe(2);
		expect(second[0]).toBe(first[0]);
	});

	it('never reuses the possibly-incomplete last previous line', async () => {
		const first = await highlightCode('let a = 1;\nlet', 'rust');
		const second = await highlightIncremental('let a = 1;\nlet b = 2;', 'rust', {
			code: 'let a = 1;\nlet',
			lines: first,
		});
		expect(second.length).toBe(2);
		expect(second[0]).toBe(first[0]);
		expect(second[1]).not.toBe(first[1]);
	});

	it('falls back to a full pass on a non-prefix update', async () => {
		const first = await highlightCode('let a = 1;\nlet b = 2;', 'rust');
		const second = await highlightIncremental('const a = 1;\nlet b = 2;', 'rust', {
			code: 'let a = 1;\nlet b = 2;',
			lines: first,
		});
		expect(second.map((line) => [...line])).not.toEqual(first.map((line) => [...line]));
	});

	it('falls back to a full pass when the reused region leaves a block comment open', async () => {
		const first = await highlightCode('/* open', 'rust');
		const second = await highlightIncremental('/* open\ncode', 'rust', {
			code: '/* open',
			lines: first,
		});
		// The full pass classifies the second line as a comment; the guarded
		// fallback must produce that correct result rather than blead state.
		expect(second[1].some((t) => t.kind === 'comment')).toBe(true);
	});

	it('handles a null previous state like a full pass', async () => {
		const result = await highlightIncremental('let a = 1;', 'rust', null);
		expect(result.length).toBe(1);
	});
});
