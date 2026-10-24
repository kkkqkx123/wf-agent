import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import { gitHunksToDiffFile } from '#lib/types/diff';
import type { DiffFile } from '#lib/types/diff';

export interface RepoMeta {
	name: string;
	root: string;
	branch: string;
	upstream: string | null;
	ahead: number;
	behind: number;
	head: string;
	bare: boolean;
	mergeHead: boolean;
	rebaseHead: boolean;
	cherryPickHead: boolean;
}

export interface GitFile {
	path: string;
	staged: string;
	unstaged: string;
	kind: string;
	renamedFrom: string | null;
}

interface DiffHunk {
	header: string;
	lines: string[];
}

interface DiffFileWire {
	path: string;
	hunks: DiffHunk[];
	additions: number;
	deletions: number;
}

export interface CommitEntry {
	id: string;
	shortId: string;
	author: string;
	email: string;
	timestamp: number;
	subject: string;
	body: string;
	parents: string[];
	refs: string[];
}

export interface BranchEntry {
	name: string;
	current: boolean;
	upstream: string | null;
	ahead: number;
	behind: number;
	commit: string;
	subject: string;
}

export interface StashEntry {
	index: number;
	name: string;
	message: string;
}

export interface CommitFileEntry {
	path: string;
	status: string;
	renamedFrom: string | null;
}

export interface RemoteEntry {
	name: string;
	url: string;
	branches: string[];
}

export interface TagEntry {
	name: string;
	commit: string;
	message: string;
}

export interface SubmoduleEntry {
	path: string;
	commit: string;
	status: string;
}

export interface WorktreeEntry {
	path: string;
	branch: string;
	commit: string;
	bare: boolean;
}

export interface ReflogEntry {
	id: string;
	shortId: string;
	selector: string;
	message: string;
	timestamp: number;
}

export interface RegistryEntry {
	id: string;
	danger: string;
	params: string[];
	available: boolean;
	reason: string | null;
}

export interface Fingerprint {
	statusHash: string;
	head: string;
	branchSummary: string;
}

export async function getMeta(): Promise<RepoMeta> {
	const data = await call<Record<string, unknown>>(
		client.GET('/api/v1/git/meta'),
	);
	const v = requireData(data, 'Git meta') as Record<string, unknown>;
	return {
		name: String(v.name ?? ''),
		root: String(v.root ?? ''),
		branch: String(v.branch ?? ''),
		upstream: (v.upstream as string | null) ?? null,
		ahead: Number(v.ahead ?? 0),
		behind: Number(v.behind ?? 0),
		head: String(v.head ?? ''),
		bare: Boolean(v.bare ?? false),
		mergeHead: Boolean(v.merge_head ?? false),
		rebaseHead: Boolean(v.rebase_head ?? false),
		cherryPickHead: Boolean(v.cherry_pick_head ?? false),
	};
}

export interface StatusCounts {
	branch: string;
	staged: number;
	unstaged: number;
	untracked: number;
	conflicted: number;
}

export async function getStatus(): Promise<StatusCounts> {
	const data = await call<Record<string, unknown>>(
		client.GET('/api/v1/git/status'),
	);
	const v = requireData(data, 'Git status') as Record<string, unknown>;
	return {
		branch: String(v.branch ?? ''),
		staged: Number(v.staged ?? 0),
		unstaged: Number(v.unstaged ?? 0),
		untracked: Number(v.untracked ?? 0),
		conflicted: Number(v.conflicted ?? 0),
	};
}

export async function listFiles(): Promise<GitFile[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/files'));
	const rows = requireData(data, 'Git files') as Array<Record<string, unknown>>;
	return (Array.isArray(rows) ? rows : []).map((f) => ({
		path: String(f.path ?? ''),
		staged: String(f.staged ?? ''),
		unstaged: String(f.unstaged ?? ''),
		kind: String(f.kind ?? ''),
		renamedFrom: (f.renamed_from as string | null) ?? null,
	}));
}

export async function getDiff(path?: string): Promise<DiffFile[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/git/diff', { params: { query: { path } } }),
	);
	const rows = requireData(data, 'Git diff') as DiffFileWire[];
	return (Array.isArray(rows) ? rows : []).map((f) =>
		gitHunksToDiffFile(f.path, f.hunks, f.additions, f.deletions),
	);
}

export async function listCommits(params?: {
	limit?: number;
	offset?: number;
	branch?: string;
}): Promise<CommitEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/git/commits', {
			params: {
				query: {
					limit: params?.limit,
					offset: params?.offset,
					branch: params?.branch,
				},
			},
		}),
	);
	const rows = requireData(data, 'Git commits') as Array<
		Record<string, unknown>
	>;
	return (Array.isArray(rows) ? rows : []).map((c) => ({
		id: String(c.id ?? ''),
		shortId: String(c.short_id ?? String(c.id ?? '').slice(0, 7)),
		author: String(c.author ?? ''),
		email: String(c.email ?? ''),
		timestamp: Number(c.timestamp ?? 0),
		subject: String(c.subject ?? ''),
		body: String(c.body ?? ''),
		parents: (c.parents as string[]) ?? [],
		refs: (c.refs as string[]) ?? [],
	}));
}

export async function listBranches(): Promise<BranchEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/branches'));
	const rows = requireData(data, 'Git branches') as BranchEntry[];
	return Array.isArray(rows) ? rows : [];
}

export async function listStash(): Promise<StashEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/stash'));
	const rows = requireData(data, 'Git stash') as StashEntry[];
	return Array.isArray(rows) ? rows : [];
}

export async function getCommitFiles(
	commit: string,
): Promise<CommitFileEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/git/commits/files', { params: { query: { commit } } }),
	);
	const rows = requireData(data, 'Git commit files') as Array<
		Record<string, unknown>
	>;
	return (Array.isArray(rows) ? rows : []).map((f) => ({
		path: String(f.path ?? ''),
		status: String(f.status ?? ''),
		renamedFrom: (f.renamed_from as string | null) ?? null,
	}));
}

export async function getDiffRange(
	from: string,
	to: string,
	path?: string,
): Promise<DiffFile[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/git/diff/range', {
			params: { query: { from, to, path } },
		}),
	);
	const rows = requireData(data, 'Git diff range') as DiffFileWire[];
	return (Array.isArray(rows) ? rows : []).map((f) =>
		gitHunksToDiffFile(f.path, f.hunks, f.additions, f.deletions),
	);
}

export async function listRemotes(): Promise<RemoteEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/remotes'));
	const rows = requireData(data, 'Git remotes') as RemoteEntry[];
	return Array.isArray(rows) ? rows : [];
}

export async function listTags(): Promise<TagEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/tags'));
	const rows = requireData(data, 'Git tags') as TagEntry[];
	return Array.isArray(rows) ? rows : [];
}

/** Linked worktrees of the user repository, read-only. */
export async function listWorktrees(): Promise<WorktreeEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/worktrees'));
	const rows = requireData(data, 'Git worktrees') as WorktreeEntry[];
	return Array.isArray(rows) ? rows : [];
}

/** Nested submodule states of the user repository, read-only. */
export async function listSubmodules(): Promise<SubmoduleEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/submodules'));
	const rows = requireData(data, 'Git submodules') as SubmoduleEntry[];
	return Array.isArray(rows) ? rows : [];
}

/** Reflog entries of the user repository. */
export async function listReflog(): Promise<ReflogEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/reflog'));
	const rows = requireData(data, 'Git reflog') as Array<
		Record<string, unknown>
	>;
	return (Array.isArray(rows) ? rows : []).map((r) => ({
		id: String(r.id ?? ''),
		shortId: String(r.short_id ?? String(r.id ?? '').slice(0, 7)),
		selector: String(r.selector ?? ''),
		message: String(r.message ?? ''),
		timestamp: Number(r.timestamp ?? 0),
	}));
}

export async function getRegistry(): Promise<RegistryEntry[]> {
	const data = await call<unknown>(client.GET('/api/v1/git/registry'));
	const rows = requireData(data, 'Git registry') as RegistryEntry[];
	return Array.isArray(rows) ? rows : [];
}

export async function getFingerprint(): Promise<Fingerprint> {
	const data = await call<Record<string, unknown>>(
		client.GET('/api/v1/git/fingerprint'),
	);
	const v = requireData(data, 'Git fingerprint') as Record<string, unknown>;
	return {
		statusHash: String(v.status_hash ?? ''),
		head: String(v.head ?? ''),
		branchSummary: String(v.branch_summary ?? ''),
	};
}

export async function runAction(
	action: string,
	params: Record<string, string>,
	confirm?: string,
): Promise<{ message: string; echo: string }> {
	const data = await call<{ message?: string; echo?: string }>(
		client.POST('/api/v1/git/action', { body: { action, params, confirm } }),
	);
	const v = requireData(data, `Git action ${action}`);
	return { message: v.message ?? '', echo: v.echo ?? '' };
}
