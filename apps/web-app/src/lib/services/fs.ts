import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';

export type FsEntry = components['schemas']['FsEntryView'];
export type FsFileContent = components['schemas']['FsFileView'];
export type FsStat = components['schemas']['FsStatView'];

export interface FsTreeEntry {
	path: string;
	is_dir: boolean;
	size: number;
}

export interface FsTreeResult {
	entries: FsTreeEntry[];
	truncated: boolean;
}

export async function listFsTree(
	base?: string,
	depth?: number,
	limit?: number,
): Promise<FsTreeResult> {
	const data = await call<unknown>(
		client.GET('/api/v1/fs/tree', {
			params: { query: { path: base ?? '', depth, limit } },
		}),
	);
	const value = requireData(data, 'FS tree') as {
		entries?: Array<Record<string, unknown>>;
		truncated?: boolean;
	};
	const entries = (Array.isArray(value.entries) ? value.entries : []).map((e) => ({
		path: String(e.path ?? ''),
		is_dir: Boolean(e.is_dir),
		size: Number(e.size ?? 0),
	}));
	return { entries, truncated: Boolean(value.truncated) };
}

export async function listFs(path?: string): Promise<FsEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/fs/list', { params: { query: { path: path ?? '' } } }),
	);
	const rows = requireData(data, 'FS list') as FsEntry[];
	return Array.isArray(rows) ? rows : [];
}

export async function statFs(path: string): Promise<FsStat> {
	const data = await call<unknown>(client.GET('/api/v1/fs/stat', { params: { query: { path } } }));
	return requireData(data, 'FS stat') as FsStat;
}

export async function readFsFile(path: string): Promise<FsFileContent> {
	const data = await call<unknown>(
		client.GET('/api/v1/fs/content', { params: { query: { path } } }),
	);
	return requireData(data, 'FS file') as FsFileContent;
}

export async function writeFsFile(
	path: string,
	content: string,
	expectedHash?: string,
): Promise<FsFileContent> {
	const data = await call<unknown>(
		client.PUT('/api/v1/fs/content', {
			params: { query: { path } },
			body: { content, expected_hash: expectedHash ?? null },
		}),
	);
	return requireData(data, 'FS write') as FsFileContent;
}

export async function createFsEntry(path: string, isDir: boolean): Promise<FsEntry> {
	const data = await call<unknown>(
		client.POST('/api/v1/fs/entries', { body: { path, is_dir: isDir } }),
	);
	return requireData(data, 'FS create') as FsEntry;
}

export async function deleteFsEntry(path: string): Promise<void> {
	await call<unknown>(client.DELETE('/api/v1/fs/entries', { params: { query: { path } } }));
}

export async function renameFsEntry(from: string, to: string): Promise<FsEntry> {
	const data = await call<unknown>(client.PATCH('/api/v1/fs/entries/move', { body: { from, to } }));
	return requireData(data, 'FS rename') as FsEntry;
}

export interface FsContentMatch {
	path: string;
	line: number;
	column: number | null;
	preview: string;
}

export interface FsContentResult {
	matches: FsContentMatch[];
	truncated: boolean;
}

export async function searchFsContent(
	query: string,
	options?: { path?: string; include?: string; limit?: number },
): Promise<FsContentResult> {
	const needle = query.trim();
	if (!needle) return { matches: [], truncated: false };
	const data = await call<unknown>(
		client.GET('/api/v1/fs/search', {
			params: {
				query: {
					query: needle,
					path: options?.path ?? '',
					include: options?.include,
					limit: options?.limit ?? 100,
				},
			},
		}),
	);
	const value = requireData(data, 'FS search') as {
		matches?: Array<Record<string, unknown>>;
		truncated?: boolean;
	};
	const matches = (Array.isArray(value.matches) ? value.matches : []).map((m) => ({
		path: String(m.path ?? ''),
		line: Number(m.line ?? 1),
		column: typeof m.column === 'number' ? (m.column as number) : null,
		preview: String(m.preview ?? ''),
	}));
	return { matches, truncated: Boolean(value.truncated) };
}

export function languageForPath(path: string): string {
	const lower = path.toLowerCase();
	const dot = lower.lastIndexOf('.');
	const ext = dot >= 0 ? lower.slice(dot + 1) : '';
	switch (ext) {
		case 'ts':
		case 'mts':
		case 'cts':
			return 'typescript';
		case 'tsx':
			return 'typescript';
		case 'js':
		case 'mjs':
		case 'cjs':
		case 'jsx':
			return 'javascript';
		case 'svelte':
			return 'svelte';
		case 'json':
		case 'jsonc':
			return 'json';
		case 'rs':
			return 'rust';
		case 'py':
			return 'python';
		case 'go':
			return 'go';
		case 'toml':
			return 'toml';
		case 'yaml':
		case 'yml':
			return 'yaml';
		case 'md':
		case 'markdown':
			return 'markdown';
		case 'html':
		case 'htm':
			return 'html';
		case 'css':
			return 'css';
		case 'sh':
		case 'bash':
			return 'shell';
		case 'sql':
			return 'sql';
		case 'xml':
			return 'xml';
		default:
			return 'plaintext';
	}
}
