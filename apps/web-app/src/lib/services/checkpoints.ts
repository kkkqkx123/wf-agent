import { client, request } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Approval, Checkpoint, FileChange } from '$lib/types/models';

interface CheckpointDto {
	id?: string;
	entity_id?: string;
	entityId?: string;
	executionId?: string;
	entity_type?: string;
	entityType?: string;
	checkpoint_type?: string;
	checkpointType?: string;
	kind?: string;
	timestamp?: number;
	createdAt?: string;
	created_at?: number;
	chain_position?: number | null;
	chainPosition?: number | null;
	sequence?: number;
	blob_size?: number | null;
	sizeBytes?: number;
	size_bytes?: number;
	status?: string;
	tags?: string[];
	actor?: string;
	note?: string;
	restorable?: boolean;
}

function toIso(value: number | string | null | undefined): string {
	if (typeof value === 'string') return value;
	if (typeof value === 'number') return new Date(value).toISOString();
	return '';
}

function toCheckpoint(d: CheckpointDto, index: number): Checkpoint {
	const sequence = d.sequence ?? d.chain_position ?? d.chainPosition ?? index;
	return {
		id: d.id ?? `ckpt-${index}`,
		executionId: d.entity_id ?? d.entityId ?? d.executionId ?? '',
		sequence,
		kind: d.kind ?? d.checkpoint_type ?? d.checkpointType ?? '',
		actor: d.actor ?? '',
		createdAt: toIso(d.createdAt ?? d.created_at ?? d.timestamp),
		sizeBytes: d.sizeBytes ?? d.blob_size ?? d.size_bytes ?? 0,
		note: d.note ?? '',
		restorable:
			d.restorable ?? String(d.status ?? '').toLowerCase() !== 'failed',
		chainPosition: d.chain_position ?? d.chainPosition ?? sequence,
		status: d.status ?? 'completed',
		tags: d.tags ?? [],
		entityType: d.entity_type ?? d.entityType ?? '',
	};
}

/** List checkpoints with optional paging. */
export async function listCheckpoints(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const data = await call<unknown>(
		request('GET', '/api/v1/checkpoints', {
			params: { query: { limit: params?.limit, offset: params?.offset } },
		}),
	);
	requireData(data, 'Checkpoint list');
	const page = extractPage<CheckpointDto>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as CheckpointDto[])
				: [];
	return {
		...page,
		items: rows.map((d, index) => toCheckpoint(d, index)),
	};
}

/** Checkpoints for one agent loop, newest first. */
export async function listLoopCheckpoints(
	loopId: string,
): Promise<Checkpoint[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/checkpoints/chain', {
			params: { path: { id: loopId } },
		}),
	);
	requireData(data, `Checkpoint chain missing for loop ${loopId}`);
	const page = extractPage<CheckpointDto>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as CheckpointDto[])
				: [];
	return rows.map((d, index) => toCheckpoint(d, index));
}

/** Checkpoint statistics. */
export async function getCheckpointStats(): Promise<object> {
	const data = await call<object>(
		client.GET('/api/v1/agent-checkpoints/stats'),
	);
	return requireData(data, 'Checkpoint statistics');
}

interface FileChangeDto {
	id?: string;
	path?: string;
	file?: string;
	change_type?: string;
	changeType?: string;
	kind?: string;
	actor?: string;
	actor_id?: string;
	at?: string;
	timestamp?: number;
	additions?: number;
	deletions?: number;
	session?: string;
	session_id?: string;
}

function toFileChange(d: FileChangeDto, index: number): FileChange {
	const raw = String(
		d.change_type ?? d.changeType ?? d.kind ?? '',
	).toLowerCase();
	const changeType = (
		raw === 'added' ||
		raw === 'modified' ||
		raw === 'deleted' ||
		raw === 'renamed'
			? raw
			: 'modified'
	) as FileChange['changeType'];
	return {
		id: d.id ?? `fc-${index}`,
		path: d.path ?? d.file ?? '',
		changeType,
		actor: d.actor ?? d.actor_id ?? '',
		at: typeof d.at === 'string' ? d.at : toIso(d.timestamp),
		additions: d.additions ?? 0,
		deletions: d.deletions ?? 0,
		session: d.session ?? d.session_id ?? '',
	};
}

/** File changes for an execution or checkpoint. */
export async function getFileChanges(params?: {
	actor?: string;
	path?: string;
	limit?: number;
	offset?: number;
}): Promise<FileChange[]> {
	return (await getFileChangesPage(params)).items;
}

/** Paged file changes with cursor state. */
export async function getFileChangesPage(params?: {
	actor?: string;
	path?: string;
	limit?: number;
	offset?: number;
}): Promise<PageResult<FileChange>> {
	const data = await call<unknown>(
		request('GET', '/api/v1/file-checkpoint/changes', {
			params: {
				query: {
					actor: params?.actor,
					path: params?.path,
					limit: params?.limit,
					offset: params?.offset,
				},
			},
		}),
	);
	const page = extractPage<FileChangeDto>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as FileChangeDto[])
				: [];
	return {
		...page,
		items: rows.map((d, index) => toFileChange(d, index)),
	};
}

interface ApprovalDto {
	id?: string;
	title?: string;
	kind?: string;
	type?: string;
	requester?: string;
	requested_at?: number;
	requestedAt?: string;
	status?: string;
	detail?: string;
	description?: string;
	execution_id?: string;
	executionId?: string;
}

/** Pending approval requests. */
export async function getApprovalRequests(
	status?: string,
): Promise<Approval[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/approvals/pending'),
	);
	const page = extractPage<ApprovalDto>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as ApprovalDto[])
				: [];
	return rows
		.map(
			(d, index) =>
				({
					id: d.id ?? `approval-${index}`,
					title: d.title ?? '',
					kind: d.kind ?? d.type ?? '',
					requester: d.requester ?? '',
					requestedAt:
						typeof d.requestedAt === 'string'
							? d.requestedAt
							: toIso(d.requested_at),
					status: d.status ?? '',
					detail: d.detail ?? d.description ?? '',
					executionId: d.execution_id ?? d.executionId ?? '',
				}) satisfies Approval,
		)
		.filter((a) => !status || a.status === status);
}

/** Restore from a checkpoint. */
export async function restoreFromCheckpoint(
	checkpointId: string,
	loopId?: string,
): Promise<boolean> {
	if (loopId) {
		await call<unknown>(
			request('POST', '/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', {
				params: { path: { id: loopId, cid: checkpointId } },
			}),
		);
		return true;
	}
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/restore', {
			params: { path: { cid: checkpointId } },
		}),
	);
	return true;
}

/** Resume execution from a checkpoint. */
export async function resumeFromCheckpoint(
	checkpointId: string,
): Promise<boolean> {
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/resume', {
			params: { path: { cid: checkpointId } },
		}),
	);
	return true;
}

export interface FileDiffLine {
	type: 'add' | 'del' | 'context' | 'meta';
	text: string;
}

export interface FileDiff {
	path: string;
	kind: string;
	lines: FileDiffLine[];
	additions: number;
	deletions: number;
	binary: boolean;
	truncated: boolean;
}

/** Rows rendered per diff; longer diffs are cut with a truncation flag. */
const DIFF_LINE_LIMIT = 400;

function parseUnifiedDiff(text: string): {
	lines: FileDiffLine[];
	truncated: boolean;
} {
	const rows = text.split('\n');
	const lines: FileDiffLine[] = [];
	for (const raw of rows) {
		if (lines.length >= DIFF_LINE_LIMIT) break;
		if (
			raw.startsWith('+++') ||
			raw.startsWith('---') ||
			raw.startsWith('diff ') ||
			raw.startsWith('index ') ||
			raw.startsWith('@@')
		) {
			lines.push({ type: 'meta', text: raw });
		} else if (raw.startsWith('+')) {
			lines.push({ type: 'add', text: raw.slice(1) });
		} else if (raw.startsWith('-')) {
			lines.push({ type: 'del', text: raw.slice(1) });
		} else if (raw.startsWith('\\')) {
			continue;
		} else {
			lines.push({
				type: 'context',
				text: raw.startsWith(' ') ? raw.slice(1) : raw,
			});
		}
	}
	return { lines, truncated: rows.length > lines.length };
}

interface FileDiffDto {
	path?: string;
	kind?: string;
	diff?: string | null;
	additions?: number | null;
	deletions?: number | null;
}

function toFileDiff(d: FileDiffDto): FileDiff {
	const text = d.diff ?? '';
	const parsed = parseUnifiedDiff(text);
	return {
		path: d.path ?? '',
		kind: String(d.kind ?? '').toLowerCase(),
		lines: parsed.lines,
		additions: d.additions ?? 0,
		deletions: d.deletions ?? 0,
		binary: !text,
		truncated: parsed.truncated,
	};
}
/** Per-file staged diffs for one actor workspace, backing the diff preview. */
export async function getStagedDiffs(actor: string): Promise<FileDiff[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/diff/staged/{id}', {
			params: { path: { id: actor } },
		}),
	);
	const rows = Array.isArray(data)
		? (data as FileDiffDto[])
		: extractPage<FileDiffDto>(data).items;
	return rows.map(toFileDiff);
}

/** Per-file diff between two actor workspaces. */
export async function getDiffActors(a: string, b: string): Promise<FileDiff[]> {
	const data = await call<unknown>(
		request('GET', '/api/v1/file-checkpoint/diff/actors/{a}/{b}', {
			params: { path: { a, b } },
		}),
	);
	const rows = Array.isArray(data)
		? (data as FileDiffDto[])
		: extractPage<FileDiffDto>(data).items;
	return rows.map(toFileDiff);
}

export interface FileContent {
	path: string;
	actor: string;
	hash: string;
	size: number;
	isBinary: boolean;
	content: string | null;
	truncated: boolean;
	timestamp: number;
}

interface FileContentDto {
	path?: string;
	actor?: string;
	hash?: string;
	size?: number;
	is_binary?: boolean;
	isBinary?: boolean;
	content?: string | null;
	truncated?: boolean;
	timestamp?: number;
}

/** Read-only file content for one actor workspace. */
export async function getFileContent(
	actor: string,
	path: string,
): Promise<FileContent> {
	const data = requireData(
		await call<FileContentDto>(
			request('GET', '/api/v1/file-checkpoint/content', {
				params: { query: { actor, path } },
			}),
		),
		`File content missing for ${path}`,
	);
	const view = data;
	return {
		path: view.path ?? path,
		actor: view.actor ?? actor,
		hash: view.hash ?? '',
		size: view.size ?? 0,
		isBinary: view.is_binary ?? view.isBinary ?? view.content == null,
		content: view.content ?? null,
		truncated: view.truncated ?? false,
		timestamp: view.timestamp ?? 0,
	};
}

export interface FileTreeEntry {
	path: string;
	hash: string;
	size: number;
	timestamp: number;
}

export interface FileTree {
	entries: FileTreeEntry[];
	truncated: boolean;
	total: number;
}

interface FileTreeDto {
	entries?: FileTreeEntry[];
	truncated?: boolean;
	total?: number;
}

/** Capped directory tree for one actor workspace. */
export async function getFileTree(
	actor: string,
	prefix?: string,
): Promise<FileTree> {
	const data = requireData(
		await call<FileTreeDto>(
			request('GET', '/api/v1/file-checkpoint/tree/{id}', {
				params: { path: { id: actor }, query: { prefix } },
			}),
		),
		`File tree missing for ${actor}`,
	);
	return {
		entries: data.entries ?? [],
		truncated: data.truncated ?? false,
		total: data.total ?? data.entries?.length ?? 0,
	};
}

export interface FileTimelineEntry {
	path: string;
	snapshotId: string;
	contentHash: string;
	timestamp: number;
	source: string;
	movedFrom?: string | null;
}

export interface FileTimeline {
	originalPath: string;
	entries: FileTimelineEntry[];
	truncated: boolean;
	total: number;
}

interface FileTimelineDto {
	original_path?: string;
	originalPath?: string;
	entries?: Array<{
		path?: string;
		snapshot_id?: string;
		snapshotId?: string;
		content_hash?: string;
		contentHash?: string;
		timestamp?: number;
		source?: string;
		moved_from?: string | null;
		movedFrom?: string | null;
	}>;
	truncated?: boolean;
	total?: number;
}

/** Version timeline for one file path, including rename history. */
export async function getFileTimeline(path: string): Promise<FileTimeline> {
	const data = requireData(
		await call<FileTimelineDto>(
			request('GET', '/api/v1/file-checkpoint/timeline/{id}', {
				params: { path: { id: path } },
			}),
		),
		`File timeline missing for ${path}`,
	);
	return {
		originalPath: data.original_path ?? data.originalPath ?? path,
		entries: (data.entries ?? []).map((entry) => ({
			path: entry.path ?? path,
			snapshotId: entry.snapshot_id ?? entry.snapshotId ?? '',
			contentHash: entry.content_hash ?? entry.contentHash ?? '',
			timestamp: entry.timestamp ?? 0,
			source: entry.source ?? '',
			movedFrom: entry.moved_from ?? entry.movedFrom ?? null,
		})),
		truncated: data.truncated ?? false,
		total: data.total ?? data.entries?.length ?? 0,
	};
}

/** Approve a pending approval request. */
export async function approveApproval(id: string): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/file-checkpoint/approvals/{id}/approve', {
			params: { path: { id } },
			body: {},
		}),
	);
}

/** Reject a pending approval request with an optional reason. */
export async function rejectApproval(
	id: string,
	reason?: string,
): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/file-checkpoint/approvals/{id}/reject', {
			params: { path: { id } },
			body: { reason: reason ?? null },
		}),
	);
}
