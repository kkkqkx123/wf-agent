/** Mock routes: workspace filesystem for the IDE preview. */

import type { MockRouter } from '../router';
import {
	allFiles,
	copyPath,
	createEntry,
	flattenTree,
	hashFor,
	listDir,
	movePath,
	readFile,
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
	router.on('GET', '/api/v1/fs/search', (_p, query) => searchWorkspace(String(query.query ?? '')));
	router.on('GET', '/api/v1/fs/content', (_p, query) => {
		const path = String(query.path ?? 'README.md');
		const content = readFile(path);
		return {
			path,
			content,
			size: content.length,
			truncated: false,
			is_binary: false,
			hash: hashFor(path),
		};
	});
	router.on('PUT', '/api/v1/fs/content', (_p, query, body) => {
		const path = String(query.path ?? 'README.md');
		const b = (body ?? {}) as { content?: string; expected_hash?: string | null };
		const result = writeFile(path, String(b.content ?? ''), b.expected_hash ?? null);
		if (!result.ok) {
			return mockError(409, 'CONFLICT', result.message);
		}
		return {
			path: result.path,
			content: result.content,
			size: result.content.length,
			truncated: false,
			is_binary: false,
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
		const content = readFile(path);
		const encoded =
			typeof btoa === 'function'
				? btoa(String.fromCharCode(...new TextEncoder().encode(content)))
				: Buffer.from(content, 'utf8').toString('base64');
		const filename = path.split('/').pop() ?? path;
		const mime = path.endsWith('.json')
			? 'application/json'
			: path.endsWith('.md')
				? 'text/markdown'
				: 'text/plain';
		return {
			path,
			filename,
			mime,
			size: content.length,
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

/** Whole-workspace content search over the live in-memory files. */
function searchWorkspace(q: string): {
	matches: Array<{ path: string; line: number; column: number | null; preview: string }>;
	truncated: boolean;
} {
	const needle = q.trim().toLowerCase();
	const matches: Array<{ path: string; line: number; column: number | null; preview: string }> = [];
	if (!needle) return { matches, truncated: false };
	for (const [path, content] of allFiles()) {
		const lines = content.split('\n');
		for (let i = 0; i < lines.length; i += 1) {
			const col = (lines[i] ?? '').toLowerCase().indexOf(needle);
			if (col >= 0) {
				matches.push({ path, line: i + 1, column: col + 1, preview: lines[i] ?? '' });
			}
		}
	}
	return { matches, truncated: false };
}
