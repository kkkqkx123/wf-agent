import { ide } from '#lib/stores/ide.svelte';
import {
	copyFsEntry,
	deleteFsEntry,
	renameFsEntry,
	uploadFsFile,
} from '#lib/services/fs';
import { invalidateFsTreeIndex } from '#lib/utils/fs-tree-index';

export function baseName(path: string): string {
	return path.split('/').pop() ?? path;
}

/** Select every currently loaded entry (used by the select-all command). */
export function selectAllLoaded(): void {
	const paths: string[] = [];
	for (const state of ide.dirs.values()) {
		for (const entry of state.entries ?? []) {
			paths.push(entry.path);
		}
	}
	ide.setSelection(paths);
}

export function parentOf(path: string): string {
	return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
}

function joinPath(dir: string, name: string): string {
	return dir ? `${dir}/${name}` : name;
}

export interface BatchFailure {
	path: string;
	error: string;
}

function errorText(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

async function refreshAfterWrite(touched: string[]): Promise<void> {
	for (const path of touched) ide.invalidateParents(path);
	invalidateFsTreeIndex();
	await ide.refreshGitStatus();
	ide.bumpGitRefresh();
}

/** Paste the session clipboard into `destDir`. Cut uses rename, copy uses the server copy. */
export async function pasteClipboard(destDir: string): Promise<{ ok: number; failed: BatchFailure[] }> {
	const clipboard = ide.clipboard;
	if (!clipboard || clipboard.paths.length === 0) return { ok: 0, failed: [] };
	let ok = 0;
	const failed: BatchFailure[] = [];
	const touched: string[] = [destDir];
	for (const source of clipboard.paths) {
		const target = joinPath(destDir, baseName(source));
		if (source === target) {
			failed.push({ path: source, error: 'Source and destination match' });
			continue;
		}
		try {
			if (clipboard.mode === 'cut') {
				await renameFsEntry(source, target);
				ide.retargetPath(source, target);
			} else {
				await copyFsEntry(source, target);
			}
			touched.push(source, target);
			ok += 1;
		} catch (e) {
			failed.push({ path: source, error: errorText(e) });
		}
	}
	await refreshAfterWrite(touched);
	if (clipboard.mode === 'cut' && ok > 0 && failed.length === 0) ide.clearClipboard();
	else if (clipboard.mode === 'cut' && ok > 0) {
		ide.setClipboard({
			mode: 'cut',
			paths: failed.map((f) => f.path),
		});
	}
	return { ok, failed };
}

/** Delete every path, closing affected tabs. Continues past individual failures. */
export async function deletePaths(paths: string[]): Promise<{ ok: number; failed: BatchFailure[] }> {
	let ok = 0;
	const failed: BatchFailure[] = [];
	const touched: string[] = [];
	for (const path of paths) {
		try {
			await deleteFsEntry(path);
			touched.push(path);
			ok += 1;
		} catch (e) {
			failed.push({ path, error: errorText(e) });
		}
	}
	if (ok > 0) ide.closePaths(paths);
	ide.clearSelection();
	await refreshAfterWrite(touched.length > 0 ? touched : ['']);
	return { ok, failed };
}

export interface UploadOutcome {
	uploaded: string[];
	conflicts: string[];
	failed: BatchFailure[];
}

/** Upload files into `destDir`. Existing paths report as conflicts unless overwrite is set. */
export async function uploadFiles(
	destDir: string,
	files: Array<File | Blob & { name?: string }>,
	options?: { overwrite?: boolean; names?: string[] },
): Promise<UploadOutcome> {
	const uploaded: string[] = [];
	const conflicts: string[] = [];
	const failed: BatchFailure[] = [];
	const touched: string[] = [destDir];
	const overwrite = options?.overwrite ?? false;
	for (let i = 0; i < files.length; i += 1) {
		const file = files[i];
		const rawName =
			options?.names?.[i] ?? (file instanceof File ? file.name : 'upload.bin');
		const clean = rawName.split('/').pop()?.trim() || 'upload.bin';
		const target = joinPath(destDir, clean);
		try {
			await uploadFsFile(target, file, overwrite);
			uploaded.push(target);
			touched.push(target);
		} catch (e) {
			const message = errorText(e);
			if (!overwrite && /already exists/i.test(message)) conflicts.push(target);
			else failed.push({ path: target, error: message });
		}
	}
	if (uploaded.length > 0) await refreshAfterWrite(touched);
	return { uploaded, conflicts, failed };
}
