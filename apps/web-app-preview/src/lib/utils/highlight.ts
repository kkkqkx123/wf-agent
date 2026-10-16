/**
 * Dependency-free per-language code highlighting.
 *
 * This is the web counterpart of the CLI `tool_output` highlighter and keeps
 * the same scope: lexical categories only (keyword, string, comment, number,
 * function and type shapes). It never does scope-accurate coloring, never
 * shells out and never blocks. Blocks are tokenized line by line with state
 * carried across lines so a comment or multiline string spanning rows colors
 * as one construct. A bounded cache keyed by language and content absorbs the
 * repeated tokenization that streaming re-renders would otherwise trigger.
 */

/** Lexical category of one highlighted span. */
export type TokenKind =
	'plain' | 'keyword' | 'string' | 'comment' | 'number' | 'function' | 'type';

/** One highlighted span: source text plus its category. */
export interface HighlightToken {
	text: string;
	kind: TokenKind;
}

/** One source line as a list of spans. */
export type HighlightLine = HighlightToken[];

const MAX_CACHE_ENTRIES = 64;
const MAX_CACHEABLE_LENGTH = 20000;

/** Normalize a language tag to the tokenizer key. */
const LANGUAGE_ALIASES: Record<string, string> = {
	rs: 'rust',
	py: 'python',
	py3: 'python',
	python3: 'python',
	js: 'javascript',
	jsx: 'javascript',
	mjs: 'javascript',
	cjs: 'javascript',
	node: 'javascript',
	ts: 'typescript',
	tsx: 'typescript',
	mts: 'typescript',
	cts: 'typescript',
	sh: 'bash',
	zsh: 'bash',
	shell: 'bash',
	ps1: 'bash',
	yml: 'yaml',
	'c++': 'cpp',
	cc: 'cpp',
	cxx: 'cpp',
	hpp: 'cpp',
	h: 'cpp',
	cpp: 'cpp',
	'c#': 'csharp',
	cs: 'csharp',
	golang: 'go',
	rb: 'ruby',
	kt: 'kotlin',
	kts: 'kotlin',
	hs: 'haskell',
	md: 'markdown',
};

export function normalizeLanguage(language: string | null | undefined): string {
	const raw = (language ?? '').trim().toLowerCase();
	return LANGUAGE_ALIASES[raw] ?? raw;
}

const RUST_KEYWORDS = [
	'as',
	'async',
	'await',
	'break',
	'const',
	'continue',
	'crate',
	'dyn',
	'else',
	'enum',
	'extern',
	'false',
	'fn',
	'for',
	'if',
	'impl',
	'in',
	'let',
	'loop',
	'match',
	'mod',
	'move',
	'mut',
	'pub',
	'ref',
	'return',
	'self',
	'static',
	'struct',
	'super',
	'trait',
	'true',
	'type',
	'union',
	'unsafe',
	'use',
	'where',
	'while',
	'u8',
	'u16',
	'u32',
	'u64',
	'usize',
	'i32',
	'i64',
	'isize',
	'bool',
	'str',
	'f32',
	'f64',
];

const PYTHON_KEYWORDS = [
	'False',
	'None',
	'True',
	'and',
	'as',
	'assert',
	'async',
	'await',
	'break',
	'class',
	'continue',
	'def',
	'del',
	'elif',
	'else',
	'except',
	'finally',
	'for',
	'from',
	'global',
	'if',
	'import',
	'in',
	'is',
	'lambda',
	'nonlocal',
	'not',
	'or',
	'pass',
	'raise',
	'return',
	'while',
	'with',
	'yield',
];

const JS_KEYWORDS = [
	'await',
	'break',
	'case',
	'catch',
	'class',
	'const',
	'continue',
	'debugger',
	'default',
	'delete',
	'do',
	'else',
	'export',
	'extends',
	'false',
	'finally',
	'for',
	'function',
	'if',
	'import',
	'in',
	'instanceof',
	'let',
	'new',
	'null',
	'return',
	'super',
	'switch',
	'this',
	'throw',
	'true',
	'try',
	'typeof',
	'undefined',
	'var',
	'void',
	'while',
	'yield',
];

const SHELL_KEYWORDS = [
	'if',
	'then',
	'else',
	'elif',
	'fi',
	'for',
	'while',
	'do',
	'done',
	'case',
	'esac',
	'function',
	'return',
	'exit',
	'export',
	'local',
];

const GO_KEYWORDS = [
	'break',
	'case',
	'chan',
	'const',
	'continue',
	'default',
	'defer',
	'else',
	'fallthrough',
	'for',
	'func',
	'go',
	'goto',
	'if',
	'import',
	'interface',
	'map',
	'package',
	'range',
	'return',
	'select',
	'struct',
	'switch',
	'type',
	'var',
];

const JAVA_KEYWORDS = [
	'abstract',
	'assert',
	'boolean',
	'break',
	'byte',
	'case',
	'catch',
	'char',
	'class',
	'continue',
	'default',
	'do',
	'double',
	'else',
	'enum',
	'extends',
	'final',
	'finally',
	'float',
	'for',
	'if',
	'implements',
	'import',
	'instanceof',
	'int',
	'long',
	'new',
	'package',
	'private',
	'protected',
	'public',
	'return',
	'short',
	'static',
	'super',
	'switch',
	'this',
	'throw',
	'throws',
	'try',
	'void',
	'while',
];

const CPP_KEYWORDS = [
	'alignas',
	'break',
	'case',
	'catch',
	'class',
	'const',
	'constexpr',
	'continue',
	'default',
	'delete',
	'do',
	'else',
	'enum',
	'explicit',
	'export',
	'for',
	'if',
	'namespace',
	'new',
	'noexcept',
	'private',
	'protected',
	'public',
	'return',
	'sizeof',
	'static',
	'struct',
	'switch',
	'template',
	'this',
	'throw',
	'try',
	'typename',
	'using',
	'virtual',
	'while',
];

const CSHARP_KEYWORDS = [
	'abstract',
	'as',
	'base',
	'break',
	'case',
	'catch',
	'class',
	'const',
	'continue',
	'do',
	'else',
	'enum',
	'finally',
	'for',
	'if',
	'namespace',
	'new',
	'private',
	'protected',
	'public',
	'return',
	'static',
	'switch',
	'this',
	'throw',
	'using',
	'var',
	'virtual',
	'void',
	'while',
];

const RUBY_KEYWORDS = [
	'alias',
	'and',
	'begin',
	'break',
	'case',
	'class',
	'def',
	'do',
	'else',
	'elsif',
	'end',
	'false',
	'for',
	'if',
	'in',
	'module',
	'next',
	'nil',
	'not',
	'or',
	'redo',
	'rescue',
	'retry',
	'return',
	'self',
	'super',
	'then',
	'true',
	'undef',
	'unless',
	'until',
	'when',
	'while',
	'yield',
];

const GENERIC_KEYWORDS = [
	'if',
	'else',
	'for',
	'while',
	'return',
	'true',
	'false',
	'null',
	'None',
	'class',
	'def',
	'fn',
];

function languageKeywords(language: string): string[] {
	switch (language) {
		case 'rust':
			return RUST_KEYWORDS;
		case 'python':
			return PYTHON_KEYWORDS;
		case 'javascript':
		case 'typescript':
			return JS_KEYWORDS;
		case 'bash':
			return SHELL_KEYWORDS;
		case 'go':
			return GO_KEYWORDS;
		case 'java':
			return JAVA_KEYWORDS;
		case 'cpp':
		case 'c':
			return CPP_KEYWORDS;
		case 'csharp':
			return CSHARP_KEYWORDS;
		case 'ruby':
			return RUBY_KEYWORDS;
		default:
			return [];
	}
}

function isKeyword(language: string, word: string): boolean {
	return (
		languageKeywords(language).includes(word) || GENERIC_KEYWORDS.includes(word)
	);
}

function lineCommentPrefix(language: string): string | null {
	switch (language) {
		case 'rust':
		case 'javascript':
		case 'typescript':
		case 'go':
		case 'java':
		case 'cpp':
		case 'c':
		case 'csharp':
		case 'swift':
		case 'kotlin':
		case 'scala':
		case 'php':
		case 'css':
			return '//';
		case 'python':
		case 'bash':
		case 'toml':
		case 'yaml':
		case 'dockerfile':
		case 'ruby':
		case 'powershell':
			return '#';
		case 'lua':
		case 'sql':
		case 'haskell':
			return '--';
		case 'html':
		case 'xml':
		case 'markdown':
			return '<!--';
		default:
			return null;
	}
}

function blockCommentDelims(
	language: string,
): { open: string; close: string } | null {
	switch (language) {
		case 'rust':
		case 'javascript':
		case 'typescript':
		case 'go':
		case 'java':
		case 'cpp':
		case 'c':
		case 'csharp':
		case 'swift':
		case 'kotlin':
		case 'scala':
		case 'php':
		case 'css':
			return { open: '/*', close: '*/' };
		case 'lua':
		case 'sql':
			return { open: '--[[', close: ']]' };
		case 'html':
		case 'xml':
		case 'markdown':
			return { open: '<!--', close: '-->' };
		default:
			return null;
	}
}

function isIdentifierStart(ch: string): boolean {
	return /[A-Za-z_$]/.test(ch);
}

function isIdentifierPart(ch: string): boolean {
	return /[A-Za-z0-9_$]/.test(ch);
}

interface ScanState {
	blockClose: string | null;
	stringClose: string | null;
}

/** Build the per-line span list for one code block. */
function scanCode(code: string, language: string): HighlightLine[] {
	const lines: HighlightLine[] = [[]];
	let current = lines[0];

	const push = (kind: TokenKind, text: string): void => {
		if (text === '') return;
		let start = 0;
		for (let i = 0; i < text.length; i += 1) {
			if (text[i] !== '\n') continue;
			const piece = text.slice(start, i);
			if (piece) current.push({ text: piece, kind });
			lines.push([]);
			current = lines[lines.length - 1];
			start = i + 1;
		}
		const tail = text.slice(start);
		if (tail) current.push({ text: tail, kind });
	};

	const state: ScanState = { blockClose: null, stringClose: null };
	const lineComment = lineCommentPrefix(language);
	const block = blockCommentDelims(language);

	let i = 0;
	while (i < code.length) {
		if (state.blockClose !== null) {
			const close = state.blockClose;
			const end = code.indexOf(close, i);
			if (end === -1) {
				push('comment', code.slice(i));
				i = code.length;
			} else {
				const stop = end + close.length;
				push('comment', code.slice(i, stop));
				i = stop;
				state.blockClose = null;
			}
			continue;
		}
		if (state.stringClose !== null) {
			const close = state.stringClose;
			const end = code.indexOf(close, i);
			if (end === -1) {
				push('string', code.slice(i));
				i = code.length;
			} else {
				const stop = end + close.length;
				push('string', code.slice(i, stop));
				i = stop;
				state.stringClose = null;
			}
			continue;
		}

		if (lineComment !== null && code.startsWith(lineComment, i)) {
			const end = code.indexOf('\n', i);
			const stop = end === -1 ? code.length : end;
			push('comment', code.slice(i, stop));
			i = stop;
			continue;
		}
		if (block !== null && code.startsWith(block.open, i)) {
			const end = code.indexOf(block.close, i + block.open.length);
			if (end === -1) {
				push('comment', code.slice(i));
				state.blockClose = block.close;
				i = code.length;
			} else {
				const stop = end + block.close.length;
				push('comment', code.slice(i, stop));
				i = stop;
			}
			continue;
		}

		const ch = code[i];

		if (ch === '"' || ch === "'") {
			const triple = code.startsWith(ch + ch + ch, i);
			if (triple) {
				const delim = ch + ch + ch;
				const end = code.indexOf(delim, i + 3);
				if (end === -1) {
					push('string', code.slice(i));
					state.stringClose = delim;
					i = code.length;
				} else {
					const stop = end + 3;
					push('string', code.slice(i, stop));
					i = stop;
				}
			} else {
				let end = i + 1;
				while (end < code.length && code[end] !== ch && code[end] !== '\n') {
					if (code[end] === '\\') end += 1;
					end += 1;
				}
				const stop = end < code.length && code[end] === ch ? end + 1 : end;
				push('string', code.slice(i, stop));
				i = stop;
			}
			continue;
		}
		if (ch === '`') {
			const end = code.indexOf('`', i + 1);
			if (end === -1) {
				push('string', code.slice(i));
				state.stringClose = '`';
				i = code.length;
			} else {
				const stop = end + 1;
				push('string', code.slice(i, stop));
				i = stop;
			}
			continue;
		}

		if (/[0-9]/.test(ch)) {
			let end = i + 1;
			while (end < code.length && /[0-9a-fA-FxX._]/.test(code[end])) end += 1;
			push('number', code.slice(i, end));
			i = end;
			continue;
		}

		if (isIdentifierStart(ch)) {
			let end = i + 1;
			while (end < code.length && isIdentifierPart(code[end])) end += 1;
			const word = code.slice(i, end);
			let kind: TokenKind = 'plain';
			if (isKeyword(language, word)) {
				kind = 'keyword';
			} else if (/^[A-Z]/.test(word)) {
				kind = 'type';
			} else {
				let lookahead = end;
				while (lookahead < code.length && code[lookahead] === ' ')
					lookahead += 1;
				if (code[lookahead] === '(') kind = 'function';
			}
			push(kind, word);
			i = end;
			continue;
		}

		push('plain', ch);
		i += 1;
	}

	return lines;
}

const cache = new Map<string, HighlightLine[]>();

/** Highlight one code block. Unknown languages tokenize with generic
 * keywords so plain prose still passes through as `plain`. */
export function highlightCode(
	code: string,
	language?: string | null,
): HighlightLine[] {
	const normalized = normalizeLanguage(language);
	if (code.length > MAX_CACHEABLE_LENGTH) {
		return scanCode(code, normalized);
	}
	const key = `${normalized}\u0000${code}`;
	const cached = cache.get(key);
	if (cached) return cached;
	const result = scanCode(code, normalized);
	cache.set(key, result);
	if (cache.size > MAX_CACHE_ENTRIES) {
		const oldest = cache.keys().next().value;
		if (oldest !== undefined) cache.delete(oldest);
	}
	return result;
}
