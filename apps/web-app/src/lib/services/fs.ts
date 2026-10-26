import { client, API_BASE_URL, resolveApiKey } from '#lib/api/client';
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

export interface FsRawView {
	path: string;
	filename: string;
	mime: string;
	size: number;
	content_base64: string;
}

interface FsEnvelope<T> {
	success: boolean;
	data?: T;
	error?: { code?: string; message?: string };
}

async function fsEnvelope<T>(path: string, init: RequestInit): Promise<T> {
	const key = resolveApiKey();
	const response = await fetch(`${API_BASE_URL}${path}`, {
		...init,
		headers: {
			'Content-Type': 'application/json',
			...(key ? { 'x-api-key': key } : {}),
			...(init.headers ?? {}),
		},
	});
	let body: FsEnvelope<T>;
	try {
		body = (await response.json()) as FsEnvelope<T>;
	} catch {
		throw new Error(`FS request failed (HTTP ${response.status})`);
	}
	if (!response.ok || !body.success) {
		throw new Error(body.error?.message ?? `FS request failed (HTTP ${response.status})`);
	}
	return requireData(body.data, 'FS response');
}

export async function copyFsEntry(from: string, to: string): Promise<FsEntry> {
	return fsEnvelope<FsEntry>('/api/v1/fs/entries/copy', {
		method: 'POST',
		body: JSON.stringify({ from, to }),
	});
}

export async function readFsRaw(path: string): Promise<FsRawView> {
	const params = new URLSearchParams({ path });
	return fsEnvelope<FsRawView>(`/api/v1/fs/raw?${params.toString()}`, { method: 'GET' });
}

function bytesToBase64(bytes: Uint8Array): string {
	let binary = '';
	const chunk = 8192;
	for (let i = 0; i < bytes.length; i += chunk) {
		binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
	}
	return btoa(binary);
}

export async function blobToBase64(blob: Blob): Promise<string> {
	const buffer = await blob.arrayBuffer();
	return bytesToBase64(new Uint8Array(buffer));
}

export async function uploadFsFile(
	path: string,
	data: Blob,
	overwrite = false,
): Promise<FsEntry> {
	const content_base64 = await blobToBase64(data);
	return fsEnvelope<FsEntry>('/api/v1/fs/upload', {
		method: 'POST',
		body: JSON.stringify({ path, content_base64, overwrite }),
	});
}

export function base64ToBlob(contentBase64: string, mime: string): Blob {
	const binary = atob(contentBase64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i += 1) {
		bytes[i] = binary.charCodeAt(i);
	}
	return new Blob([bytes.buffer as ArrayBuffer], { type: mime || 'application/octet-stream' });
}

export function downloadFsBlob(filename: string, mime: string, contentBase64: string): void {
	const blob = base64ToBlob(contentBase64, mime);
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = filename;
	anchor.click();
	URL.revokeObjectURL(url);
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

export interface FsContentSearchOptions {
	path?: string;
	include?: string;
	caseSensitive?: boolean;
	regex?: boolean;
	wholeWord?: boolean;
	offset?: number;
	limit?: number;
}

export async function searchFsContent(
	query: string,
	options?: FsContentSearchOptions,
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
					case_sensitive: options?.caseSensitive,
					regex: options?.regex,
					whole_word: options?.wholeWord,
					offset: options?.offset,
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
