/**
 * Web frontend syntax highlighting.
 *
 * Token boundaries come from Shiki's TextMate grammars (JavaScript regex
 * engine, no WASM assets). The fixed github-light theme is only a
 * tokenization carrier: its colors never reach the DOM. Rendering colors are
 * the application's own `text-code-*` CSS classes, which follow the app
 * theme (light/dark) automatically.
 */

import { getSingletonHighlighter, createJavaScriptRegexEngine } from 'shiki';

import { LANGUAGE_ALIASES } from './language-aliases.generated';

export type TokenKind = 'plain' | 'keyword' | 'string' | 'comment' | 'number' | 'function' | 'type';

export interface HighlightToken {
	text: string;
	kind: TokenKind;
}

export type HighlightLine = HighlightToken[];

const MAX_CACHE_ENTRIES = 64;
const MAX_CACHEABLE_LENGTH = 20000;

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

/**
 * Scope patterns relate to a scope selector by segment-prefix compatibility
 * (one being a prefix of the other): a theme rule applies to every token
 * scope that extends it. Rules are evaluated in priority order, so a rule
 * scope covering several kinds resolves to the highest-priority one -
 * `comment` first, then string-like, number, type entities, keywords, and
 * function entities.
 */
const SCOPE_RULES: Array<{ kind: TokenKind; patterns: string[][] }> = [
	{
		kind: 'comment',
		patterns: [['comment']],
	},
	{
		kind: 'string',
		patterns: [['string'], ['char'], ['regexp']],
	},
	{
		kind: 'number',
		patterns: [['constant', 'numeric']],
	},
	{
		kind: 'type',
		patterns: [
			['entity', 'name', 'type'],
			['entity', 'name', 'class'],
			['entity', 'name', 'struct'],
			['entity', 'name', 'enum'],
			['entity', 'name', 'trait'],
			['entity', 'name', 'interface'],
			['support', 'type'],
		],
	},
	{
		kind: 'keyword',
		patterns: [['keyword'], ['storage'], ['constant', 'language']],
	},
	{
		kind: 'function',
		patterns: [['entity', 'name', 'function'], ['support', 'function']],
	},
];

function scopePrefixCompatible(segments: string[], pattern: string[]): boolean {
	const shared = Math.min(segments.length, pattern.length);
	for (let index = 0; index < shared; index++) {
		if (segments[index] !== pattern[index]) return false;
	}
	return true;
}

/**
 * Classify one TextMate scope selector (as found in a theme rule) against
 * the ordered SCOPE_RULES; null when no rule applies and the token kind
 * stays plain. Exported for tests.
 */
export function ruleScopeKind(scope: string): TokenKind | null {
	const selectors = scope.toLowerCase().split(/\s+/);
	for (const rule of SCOPE_RULES) {
		for (const selector of selectors) {
			const segments = selector.split('.');
			if (rule.patterns.some((pattern) => scopePrefixCompatible(segments, pattern))) {
				return rule.kind;
			}
		}
	}
	return null;
}

function plainLines(code: string): HighlightLine[] {
	return code.split('\n').map((line) => [{ text: line, kind: 'plain' as TokenKind }]);
}

const cache = new Map<string, HighlightLine[]>();

const HIGHLIGHT_THEME = 'github-light';

/**
 * Foreground color -> token kind, derived once from the theme's own scope
 * rules. A rule scope and a SCOPE_RULES pattern relate by prefix
 * compatibility (one being a prefix of the other), because a theme rule
 * applies to every token scope that extends it. When one color covers
 * several kinds, the higher-priority SCOPE_RULES kind wins - the theme
 * itself renders those tokens identically, so the collapse mirrors the
 * visual presentation (notably: declaration names share one color, mapped
 * to 'type'). Colors equal to the default foreground stay unmapped and
 * classify as plain.
 */
let colorKindsPromise: Promise<Map<string, TokenKind>> | null = null;

function buildColorKinds(highlighter: Awaited<typeof highlighterPromise>): Map<string, TokenKind> {
	const theme = highlighter.getTheme(HIGHLIGHT_THEME);
	const map = new Map<string, TokenKind>();
	const defaultFg = theme.fg?.toLowerCase();
	for (const rule of theme.settings) {
		const foreground = rule.settings?.foreground?.toLowerCase();
		if (!foreground || foreground === defaultFg) continue;
		if (map.has(foreground)) continue;
		const scopes = Array.isArray(rule.scope) ? rule.scope : rule.scope ? [rule.scope] : [];
		for (const scope of scopes) {
			const kind = ruleScopeKind(scope);
			if (kind && kind !== 'plain') {
				map.set(foreground, kind);
				break;
			}
		}
	}
	return map;
}

function colorKinds(): Promise<Map<string, TokenKind>> {
	colorKindsPromise ??= highlighterPromise.then(buildColorKinds);
	return colorKindsPromise;
}

/** Tokenize with Shiki (no explanation - measured ~14x slower) and classify
 * every token through the theme color map. */
async function tokenize(code: string, normalized: string): Promise<HighlightLine[]> {
	const lang = await ensureLanguage(normalized);
	if (!lang) return plainLines(code);
	const [highlighter, kinds] = await Promise.all([highlighterPromise, colorKinds()]);
	const result = highlighter.codeToTokens(code, {
		lang: lang as never,
		theme: HIGHLIGHT_THEME,
	});
	return result.tokens.map((line) =>
		line.map((token) => ({
			text: token.content,
			kind: kinds.get(token.color?.toLowerCase() ?? '') ?? 'plain',
		})),
	);
}

export async function highlightCode(code: string, language?: string | null): Promise<HighlightLine[]> {
	const normalized = normalizeLanguage(language);
	if (code.length > MAX_CACHEABLE_LENGTH) {
		return tokenize(code, normalized);
	}
	const key = `${normalized}\u0000${code}`;
	const cached = cache.get(key);
	if (cached) return cached;
	const result = await tokenize(code, normalized);
	cache.set(key, result);
	if (cache.size > MAX_CACHE_ENTRIES) {
		const oldest = cache.keys().next().value;
		if (oldest !== undefined) cache.delete(oldest);
	}
	return result;
}

/** Heuristic multiline-construct guard: when the reused region leaves a block
 * comment, triple-quoted string or template literal unterminated, the tail
 * would tokenize in the wrong state, so callers fall back to a full pass. */
function hasOpenMultilineConstruct(text: string): boolean {
	const count = (pattern: RegExp) => (text.match(pattern) ?? []).length;
	if (count(/\/\*/g) > count(/\*\//g)) return true;
	if (count(/`/g) % 2 === 1) return true;
	for (const quote of ['"""', "'''"]) {
		if (count(new RegExp(quote, 'g')) % 2 === 1) return true;
	}
	return false;
}

export interface HighlightIncrementalState {
	code: string;
	lines: HighlightLine[];
}

/**
 * Streaming-oriented highlight entry point. When the previous code is a
 * prefix of the new code, every previously completed line is reused verbatim
 * and only the appended tail is tokenized; TextMate tokenization is causal
 * left-to-right, so earlier completed lines cannot change. The last previous
 * line is never reused because it may still be incomplete. Any structural
 * surprise (non-prefix update, unknown previous shape, open multiline
 * construct in the reused region) falls back to the full cached path.
 */
export async function highlightIncremental(
	code: string,
	language: string | null | undefined,
	previous: HighlightIncrementalState | null,
): Promise<HighlightLine[]> {
	const normalized = normalizeLanguage(language);
	const reusable = previous !== null && code.startsWith(previous.code);
	if (reusable && previous !== null) {
		const previousLineCount = previous.code.split('\n').length;
		const shapesMatch = previous.lines.length === previousLineCount;
		const completeCount = previous.code.endsWith('\n') ? previousLineCount : previousLineCount - 1;
		if (shapesMatch && completeCount > 0) {
			const reused = previous.lines.slice(0, completeCount);
			const reusedText = previous.code.split('\n').slice(0, completeCount).join('\n');
			if (!hasOpenMultilineConstruct(reusedText)) {
				const tail = code.split('\n').slice(completeCount).join('\n');
				// An empty tail means the append was only a newline character;
				// tokenizing it would fabricate an extra blank line.
				if (tail === '') return reused;
				const tailLines = await tokenize(tail, normalized);
				return [...reused, ...tailLines];
			}
		}
	}
	return highlightCode(code, language);
}
