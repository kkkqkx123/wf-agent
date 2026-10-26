/**
 * Web frontend syntax highlighting.
 *
 * Classification is provided by Shiki's TextMate grammars; rendering still
 * uses the application's own token CSS classes. Shiki runs with the
 * JavaScript regex engine so no WASM assets are required.
 */

import { getSingletonHighlighter, createJavaScriptRegexEngine } from 'shiki';

export type TokenKind = 'plain' | 'keyword' | 'string' | 'comment' | 'number' | 'function' | 'type';

export interface HighlightToken {
	text: string;
	kind: TokenKind;
}

export type HighlightLine = HighlightToken[];

const MAX_CACHE_ENTRIES = 64;
const MAX_CACHEABLE_LENGTH = 20000;

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
	shellscript: 'bash',
	ps1: 'powershell',
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

const SHIKI_LANGUAGE_ALIASES: Record<string, string> = {
	bash: 'shellscript',
	sh: 'shellscript',
	zsh: 'shellscript',
	shell: 'shellscript',
};

function shikiLanguage(normalized: string): string {
	return SHIKI_LANGUAGE_ALIASES[normalized] ?? normalized;
}

const highlighterPromise = getSingletonHighlighter({
	themes: ['github-light'],
	langs: [],
	engine: createJavaScriptRegexEngine(),
});

const loadedLanguages = new Set<string>();

async function ensureLanguage(normalized: string): Promise<string | null> {
	const lang = shikiLanguage(normalized);
	if (loadedLanguages.has(lang)) return lang;
	const highlighter = await highlighterPromise;
	try {
		await highlighter.loadLanguage(lang as never);
		loadedLanguages.add(lang);
		return lang;
	} catch {
		return null;
	}
}

function isIdentifier(text: string): boolean {
	return /^[A-Za-z_$]/.test(text.trim());
}

function classifyToken(token: { content: string; explanation?: Array<{ scopes: Array<{ scopeName: string }> }> }): TokenKind {
	const scopes = (token.explanation ?? []).flatMap((entry) => entry.scopes.map((scope) => scope.scopeName));
	const joined = scopes.join(' ').toLowerCase();
	if (joined.includes('comment')) return 'comment';
	if (joined.includes('string') || joined.includes('char') || joined.includes('regexp')) return 'string';
	if (joined.includes('constant.numeric') || /\bnumber\b/.test(joined)) return 'number';
	const isTypeEntity =
		joined.includes('entity.name.type') ||
		joined.includes('entity.name.class') ||
		joined.includes('entity.name.struct') ||
		joined.includes('entity.name.enum') ||
		joined.includes('entity.name.trait') ||
		joined.includes('entity.name.interface') ||
		joined.includes('support.type');
	if (joined.includes('keyword') || joined.includes('storage') || joined.includes('constant.language')) {
		if (!isTypeEntity) return 'keyword';
	}
	if (isIdentifier(token.content) && isTypeEntity) return 'type';
	if (isIdentifier(token.content) && (joined.includes('entity.name.function') || joined.includes('support.function'))) {
		return 'function';
	}
	return 'plain';
}

function plainLines(code: string): HighlightLine[] {
	return code.split('\n').map((line) => [{ text: line, kind: 'plain' as TokenKind }]);
}

const cache = new Map<string, HighlightLine[]>();

export async function highlightCode(code: string, language?: string | null): Promise<HighlightLine[]> {
	const normalized = normalizeLanguage(language);
	if (code.length > MAX_CACHEABLE_LENGTH) {
		const lang = await ensureLanguage(normalized);
		if (!lang) return plainLines(code);
		const highlighter = await highlighterPromise;
		const result = highlighter.codeToTokens(code, {
			lang: lang as never,
			theme: 'github-light',
			includeExplanation: 'scopeName',
		});
		return result.tokens.map((line) =>
			line.map((token) => ({ text: token.content, kind: classifyToken(token as never) })),
		);
	}
	const key = `${normalized}\u0000${code}`;
	const cached = cache.get(key);
	if (cached) return cached;
	const lang = await ensureLanguage(normalized);
	if (!lang) {
		const result = plainLines(code);
		cache.set(key, result);
		return result;
	}
	const highlighter = await highlighterPromise;
	const resultTokens = highlighter.codeToTokens(code, {
		lang: lang as never,
		theme: 'github-light',
		includeExplanation: 'scopeName',
	});
	const result = resultTokens.tokens.map((line) =>
		line.map((token) => ({ text: token.content, kind: classifyToken(token as never) })),
	);
	cache.set(key, result);
	if (cache.size > MAX_CACHE_ENTRIES) {
		const oldest = cache.keys().next().value;
		if (oldest !== undefined) cache.delete(oldest);
	}
	return result;
}
