/** Mock routes: workspace filesystem for the IDE preview. */

import type { MockRouter } from '../router';

const DIRS: Record<string, Array<{ name: string; isDir: boolean; size: number }>> = {
	'': [
		{ name: 'apps', isDir: true, size: 0 },
		{ name: 'crates', isDir: true, size: 0 },
		{ name: 'docs', isDir: true, size: 0 },
		{ name: 'README.md', isDir: false, size: 512 },
	],
	apps: [{ name: 'web-app', isDir: true, size: 0 }],
	'apps/web-app': [
		{ name: 'src', isDir: true, size: 0 },
		{ name: 'package.json', isDir: false, size: 1024 },
	],
	'docs/plan': [{ name: 'lightweight-ide-design.md', isDir: false, size: 2048 }],
};

const FILES: Record<string, string> = {
	'README.md': '# wf-agent (preview)\n\nRead-only preview of the workspace filesystem.\n',
	'apps/web-app/package.json': '{\n  "name": "@wf-agent/web-app-frontend"\n}\n',
	'docs/plan/lightweight-ide-design.md':
		'# Lightweight IDE (preview)\n\nFile editing in preview is mocked.\n',
};

function entriesFor(path: string) {
	const key = path.replace(/\/$/, '');
	const rows = DIRS[key] ?? [];
	return rows.map((e) => ({
		name: e.name,
		path: key ? `${key}/${e.name}` : e.name,
		is_dir: e.isDir,
		size: e.size,
		modified_ms: 1_700_000_000_000,
	}));
}

export function registerFs(router: MockRouter): void {
	router.on('GET', '/api/v1/fs/list', (_p, query) => {
		const path = String(query.path ?? '');
		return entriesFor(path);
	});
	router.on('GET', '/api/v1/fs/tree', (_p, query) => {
		const base = String(query.path ?? '');
		const rawLimit = Number((query as Record<string, unknown>).limit ?? 2000);
		const limit = Number.isFinite(rawLimit) ? Math.min(5000, Math.max(1, Math.floor(rawLimit))) : 2000;
		const flat: Array<{ path: string; is_dir: boolean; size: number }> = [];
		const visit = (prefix: string): boolean => {
			for (const row of entriesFor(prefix)) {
				flat.push({ path: row.path, is_dir: row.is_dir, size: row.size });
				if (flat.length >= limit) return true;
				if (row.is_dir && visit(row.path)) return true;
			}
			return false;
		};
		const truncated = visit(base.replace(/\/$/, ''));
		return { entries: flat.slice(0, limit), truncated };
	});
	router.on('GET', '/api/v1/fs/stat', (_p, query) => {
		const path = String(query.path ?? '');
		if (!path) {
			return { path: '', is_dir: true, size: 0, modified_ms: 1_700_000_000_000, is_binary: null };
		}
		const content = FILES[path];
		if (content !== undefined) {
			return {
				path,
				is_dir: false,
				size: content.length,
				modified_ms: 1_700_000_000_000,
				is_binary: false,
			};
		}
		if (DIRS[path] !== undefined) {
			return {
				path,
				is_dir: true,
				size: 0,
				modified_ms: 1_700_000_000_000,
				is_binary: null,
			};
		}
		return {
			path,
			is_dir: false,
			size: 128,
			modified_ms: 1_700_000_000_000,
			is_binary: false,
		};
	});
	router.on('GET', '/api/v1/fs/content', (_p, query) => {
		const path = String(query.path ?? 'README.md');
		const content = FILES[path] ?? `# Preview file: ${path}\n\nMock content for the IDE preview.\n`;
		return {
			path,
			content,
			size: content.length,
			truncated: false,
			is_binary: false,
			hash: 'preview-hash',
		};
	});
	router.on('PUT', '/api/v1/fs/content', (_p, query, body) => {
		const path = String(query.path ?? 'README.md');
		const content = String((body as { content?: string } | undefined)?.content ?? '');
		return {
			path,
			content,
			size: content.length,
			truncated: false,
			is_binary: false,
			hash: 'preview-hash-written',
		};
	});
	router.on('POST', '/api/v1/fs/entries', (_p, _q, body) => {
		const b = (body ?? {}) as { path?: string; is_dir?: boolean };
		const path = String(b.path ?? 'preview.txt');
		const name = path.split('/').pop() ?? path;
		return { name, path, is_dir: Boolean(b.is_dir), size: 0, modified_ms: 1_700_000_000_000 };
	});
	router.on('DELETE', '/api/v1/fs/entries', (_p, query) => ({
		removed: true,
		path: String(query.path ?? ''),
	}));
	router.on('PATCH', '/api/v1/fs/entries/move', (_p, _q, body) => {
		const b = (body ?? {}) as { to?: string };
		const path = String(b.to ?? 'preview.txt');
		const name = path.split('/').pop() ?? path;
		return { name, path, is_dir: false, size: 0, modified_ms: 1_700_000_000_000 };
	});
}
