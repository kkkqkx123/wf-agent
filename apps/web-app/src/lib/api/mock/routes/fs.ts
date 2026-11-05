/** Mock routes: workspace filesystem for the IDE preview. */

import type { MockRouter } from '../router';
import {
	allFiles,
	copyPath,
	createEntry,
	fileView,
	flattenTree,
	listDir,
	movePath,
	removePath,
	statPath,
	storeUpload,
	writeFile,
} from '../workspace';

/** Error marker the preview client turns into a real error envelope. */
export interface MockRouteError {
	status: number;
	code: string;
	message: string;
}

export function mockError(status: number, code: string, message: string): MockRouteError {
	return { __mockError: true, status, code, message } as unknown as MockRouteError;
}

function mimeFor(path: string): string {
	const lower = path.toLowerCase();
	if (lower.endsWith('.json') || lower.endsWith('.jsonc')) return 'application/json';
	if (lower.endsWith('.md') || lower.endsWith('.markdown')) return 'text/markdown';
	if (lower.endsWith('.html') || lower.endsWith('.htm')) return 'text/html';
	if (lower.endsWith('.css')) return 'text/css';
	if (lower.endsWith('.js') || lower.endsWith('.mjs') || lower.endsWith('.cjs'))
		return 'text/javascript';
	if (lower.endsWith('.svg')) return 'image/svg+xml';
	if (lower.endsWith('.png')) return 'image/png';
	if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) return 'image/jpeg';
	if (lower.endsWith('.gif')) return 'image/gif';
	if (lower.endsWith('.ico')) return 'image/x-icon';
	if (lower.endsWith('.pdf')) return 'application/pdf';
	return 'text/plain';
}

function asBool(value: unknown): boolean {
	if (typeof value === 'boolean') return value;
	if (typeof value === 'number') return value !== 0;
	if (typeof value === 'string') {
		const lower = value.trim().toLowerCase();
		return lower === 'true' || lower === '1' || lower === 'yes';
	}
	return false;
}

function asInt(value: unknown, fallback: number): number {
	const n = typeof value === 'number' ? value : Number(value);
	return Number.isFinite(n) ? Math.floor(n) : fallback;
}

/** Match a workspace path against a comma/space separated `include` glob. */
function matchesInclude(path: string, include: string): boolean {
	const raw = include.trim();
	if (!raw) return true;
	const lowerPath = path.toLowerCase();
	for (const part of raw.split(/[,;\s]+/).filter(Boolean)) {
		const token = part.trim().toLowerCase();
		if (!token) continue;
		if (!token.includes('*')) {
			if (lowerPath.endsWith(token) || lowerPath.includes(token)) return true;
			continue;
		}
		const regex = new RegExp(
			`^${token.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*')}$`,
		);
		if (regex.test(lowerPath) || regex.test(lowerPath.split('/').pop() ?? '')) return true;
	}
	return false;
}

/** File is inside `scope` (prefix dir) or the scope itself. */
function inScope(path: string, scope: string): boolean {
	const clean = scope.trim().replace(/\/+$/, '');
	if (!clean) return true;
	return path === clean || path.startsWith(`${clean}/`);
}

export function registerFs(router: MockRouter): void {
	router.on('GET', '/api/v1/fs/list', (_p, query) => listDir(String(query.path ?? '')));
	router.on('GET', '/api/v1/fs/tree', (_p, query) => {
		const base = String(query.path ?? '');
		const rawLimit = Number((query as Record<string, unknown>).limit ?? 2000);
		const limit = Number.isFinite(rawLimit)
			? Math.min(5000, Math.max(1, Math.floor(rawLimit)))
			: 2000;
		return flattenTree(base, limit);
	});
	router.on('GET', '/api/v1/fs/stat', (_p, query) => statPath(String(query.path ?? '')));
	router.on('GET', '/api/v1/fs/search', (_p, query) =>
		searchWorkspace({
			query: String(query.query ?? ''),
			scope: String(query.path ?? ''),
			include: String(query.include ?? ''),
			caseSensitive: asBool(query.case_sensitive),
			regex: asBool(query.regex),
			wholeWord: asBool(query.whole_word),
			offset: asInt(query.offset, 0),
			limit: asInt(query.limit, 100),
		}),
	);
	router.on('GET', '/api/v1/fs/content', (_p, query) => {
		const path = String(query.path ?? 'README.md');
		const view = fileView(path);
		return {
			path,
			content: view.content,
			size: view.size,
			truncated: view.truncated,
			is_binary: view.isBinary,
			hash: view.hash,
		};
	});
	router.on('PUT', '/api/v1/fs/content', (_p, query, body) => {
		const path = String(query.path ?? 'README.md');
		const b = (body ?? {}) as { content?: string; expected_hash?: string | null };
		const result = writeFile(path, String(b.content ?? ''), b.expected_hash ?? null);
		if (!result.ok) {
			return mockError(409, 'CONFLICT', result.message);
		}
		const view = fileView(result.path);
		return {
			path: result.path,
			content: result.content,
			size: result.content.length,
			truncated: view.truncated,
			is_binary: view.isBinary,
			hash: result.hash,
		};
	});
	router.on('POST', '/api/v1/fs/entries', (_p, _q, body) => {
		const b = (body ?? {}) as { path?: string; is_dir?: boolean };
		return createEntry(String(b.path ?? 'preview.txt'), Boolean(b.is_dir));
	});
	router.on('DELETE', '/api/v1/fs/entries', (_p, query) => ({
		removed: removePath(String(query.path ?? '')),
		path: String(query.path ?? ''),
	}));
	router.on('PATCH', '/api/v1/fs/entries/move', (_p, _q, body) => {
		const b = (body ?? {}) as { from?: string; to?: string };
		const moved = movePath(String(b.from ?? ''), String(b.to ?? 'preview.txt'));
		if (!moved) return mockError(404, 'NOT_FOUND', `Preview path not found: ${b.from ?? ''}`);
		return moved;
	});
	router.on('POST', '/api/v1/fs/entries/copy', (_p, _q, body) => {
		const b = (body ?? {}) as { from?: string; to?: string };
		const copied = copyPath(String(b.from ?? ''), String(b.to ?? 'preview-copy.txt'));
		if (!copied) return mockError(404, 'NOT_FOUND', `Preview path not found: ${b.from ?? ''}`);
		return copied;
	});
	router.on('GET', '/api/v1/fs/raw', (_p, query) => {
		const path = String(query.path ?? '');
		const view = fileView(path);
		const encoded =
			typeof btoa === 'function'
				? btoa(String.fromCharCode(...new TextEncoder().encode(view.content)))
				: Buffer.from(view.content, 'utf8').toString('base64');
		const filename = path.split('/').pop() ?? path;
		return {
			path,
			filename,
			mime: mimeFor(path),
			size: view.size,
			content_base64: encoded,
		};
	});
	router.on('POST', '/api/v1/fs/upload', (_p, _q, body) => {
		const b = (body ?? {}) as { path?: string; content_base64?: string; overwrite?: boolean };
		const path = String(b.path ?? 'preview-upload.txt');
		const overwrite = b.overwrite === true;
		if (!overwrite && allFiles().has(path)) {
			return mockError(409, 'ALREADY_EXISTS', `${path} already exists in preview workspace`);
		}
		let content = '';
		if (b.content_base64 && typeof atob === 'function') {
			content = atob(b.content_base64);
		}
		return storeUpload(path, content);
	});
}

export interface ContentSearchOptions {
	query: string;
	scope: string;
	include: string;
	caseSensitive: boolean;
	regex: boolean;
	wholeWord: boolean;
	offset: number;
	limit: number;
}

function escapeRegExp(text: string): string {
	return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Build the line matcher for literal / regex / whole-word searches. */
function buildMatcher(
	query: string,
	caseSensitive: boolean,
	regex: boolean,
	wholeWord: boolean,
): ((line: string) => number) | null {
	const needle = query.trim();
	if (!needle) return null;
	if (regex) {
		try {
			const flags = caseSensitive ? 'g' : 'gi';
			const pattern = wholeWord ? `\\b(?:${needle})\\b` : needle;
			const re = new RegExp(pattern, flags);
			return (line: string) => {
				re.lastIndex = 0;
				const hit = re.exec(line);
				return hit ? hit.index : -1;
			};
		} catch {
			return null;
		}
	}
	if (wholeWord) {
		try {
			const re = new RegExp(`\\b${escapeRegExp(needle)}\\b`, caseSensitive ? 'g' : 'gi');
			return (line: string) => {
				re.lastIndex = 0;
				const hit = re.exec(line);
				return hit ? hit.index : -1;
			};
		} catch {
			return null;
		}
	}
	if (caseSensitive) {
		return (line: string) => line.indexOf(needle);
	}
	const lower = needle.toLowerCase();
	return (line: string) => line.toLowerCase().indexOf(lower);
}

/**
 * Whole-workspace content search over the live in-memory files.
 *
 * Mirrors the service options (scope path, include globs, case/regex/word
 * flags and offset/limit pagination) so the content-search tab, its
 * grouping and its load-more flow render like the backend in preview.
 */
function searchWorkspace(options: ContentSearchOptions): {
	matches: Array<{ path: string; line: number; column: number | null; preview: string }>;
	truncated: boolean;
} {
	const matcher = buildMatcher(
		options.query,
		options.caseSensitive,
		options.regex,
		options.wholeWord,
	);
	if (!matcher) return { matches: [], truncated: false };
	const offset = Math.max(0, options.offset);
	const limit = Math.min(500, Math.max(1, options.limit));
	const matches: Array<{ path: string; line: number; column: number | null; preview: string }> =
		[];
	for (const [path, content] of allFiles()) {
		if (!inScope(path, options.scope)) continue;
		if (!matchesInclude(path, options.include)) continue;
		const lines = content.split('\n');
		for (let i = 0; i < lines.length; i += 1) {
			let col: number;
			try {
				col = matcher(lines[i] ?? '');
			} catch {
				continue;
			}
			if (col >= 0) {
				matches.push({ path, line: i + 1, column: col + 1, preview: lines[i] ?? '' });
			}
		}
	}
	matches.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
	const window = matches.slice(offset, offset + limit);
	return { matches: window, truncated: offset + window.length < matches.length };
}
