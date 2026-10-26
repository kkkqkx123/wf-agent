import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';

export type FsEntry = components['schemas']['FsEntryView'];
export type FsFileContent = components['schemas']['FsFileView'];
export type FsStat = components['schemas']['FsStatView'];

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
