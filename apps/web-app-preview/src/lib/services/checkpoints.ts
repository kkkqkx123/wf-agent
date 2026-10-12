import { client } from '$lib/api/client';
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
		client.GET('/api/v1/checkpoints', {
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
		client.GET('/api/v1/file-checkpoint/changes', {
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
			client.POST('/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', {
				params: { path: { id: loopId, cid: checkpointId } },
			}),
		);
		return true;
	}
	await call<unknown>(
		client.POST('/api/v1/executions/checkpoints/{cid}/restore', {
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
		client.POST('/api/v1/executions/checkpoints/{cid}/resume', {
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
/** Per-file diffs of one actor workspace against main, backing the diff preview. */
export async function getMainDiffs(actor: string): Promise<FileDiff[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/diff/main/{id}', {
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
		client.GET('/api/v1/file-checkpoint/diff/actors/{a}/{b}', {
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
			client.GET('/api/v1/file-checkpoint/content', {
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
			client.GET('/api/v1/file-checkpoint/tree/{id}', {
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
			client.GET('/api/v1/file-checkpoint/timeline/{id}', {
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
		client.POST('/api/v1/file-checkpoint/approvals/{id}/approve', {
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
		client.POST('/api/v1/file-checkpoint/approvals/{id}/reject', {
			params: { path: { id } },
			body: { reason: reason ?? null },
		}),
	);
}

/** Delete an execution checkpoint by id. */
export async function deleteCheckpoint(id: string): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/checkpoints/{id}', {
			params: { path: { id } },
		}),
	);
}

export interface PartitionView {
	partitionId: string;
	name: string;
	kind: string;
	actor: string | null;
	currentSnapshot: string;
	historyLen: number;
	createdAt: number;
	updatedAt: number;
}

interface PartitionDto {
	partition_id?: string;
	partitionId?: string;
	name?: string;
	kind?: string;
	actor?: string | null;
	current_snapshot?: string;
	currentSnapshot?: string;
	history_len?: number;
	historyLen?: number;
	created_at?: number;
	createdAt?: number;
	updated_at?: number;
	updatedAt?: number;
}

/** All partitions of the file-checkpoint store. */
export async function listPartitions(): Promise<PartitionView[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/partitions'),
	);
	const rows = Array.isArray(data)
		? (data as PartitionDto[])
		: extractPage<PartitionDto>(data).items;
	return rows.map((d) => ({
		partitionId: d.partition_id ?? d.partitionId ?? '',
		name: d.name ?? '',
		kind: d.kind ?? '',
		actor: d.actor ?? null,
		currentSnapshot: d.current_snapshot ?? d.currentSnapshot ?? '',
		historyLen: d.history_len ?? d.historyLen ?? 0,
		createdAt: d.created_at ?? d.createdAt ?? 0,
		updatedAt: d.updated_at ?? d.updatedAt ?? 0,
	}));
}

export interface ConflictRegion {
	startLine: number;
	endLine: number;
}

export interface ConflictFile {
	path: string;
	snapshotId: string;
	partition: string;
	conflictCount: number;
}

interface ConflictDto {
	path?: string;
	snapshot_id?: string;
	snapshotId?: string;
	partition?: string;
	conflicts?: unknown[];
}

/** Files with unresolved merge conflicts, enumerated from unresolved merge commits. */
export async function listConflicts(): Promise<ConflictFile[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/conflicts'),
	);
	const rows = Array.isArray(data)
		? (data as ConflictDto[])
		: extractPage<ConflictDto>(data).items;
	return rows.map((d) => ({
		path: d.path ?? '',
		snapshotId: d.snapshot_id ?? d.snapshotId ?? '',
		partition: d.partition ?? '',
		conflictCount: Array.isArray(d.conflicts) ? d.conflicts.length : 0,
	}));
}

/** Resolve conflicts on a feature ref with full-file resolved contents. */
export async function resolveConflicts(
	actor: string,
	feature: string,
	resolutions: Record<string, string>,
	session?: string,
): Promise<{ remaining: number }> {
	const data = await call<{ remaining?: number }>(
		client.POST('/api/v1/file-checkpoint/conflicts/resolve', {
			body: { actor, feature, resolutions, session: session ?? null },
		}),
	);
	const view = requireData(data, 'Conflict resolution');
	return { remaining: view.remaining ?? 0 };
}

export interface EditSession {
	id: string;
	label: string | null;
	createdAt: number;
}

interface EditSessionDto {
	id?: string;
	label?: string | null;
	created_at?: number;
	createdAt?: number;
}

function toEditSession(d: EditSessionDto): EditSession {
	return {
		id: typeof d.id === 'string' ? d.id : String(d.id ?? ''),
		label: d.label ?? null,
		createdAt: d.created_at ?? d.createdAt ?? 0,
	};
}

/** Staged edit sessions (newest first). */
export async function listEditSessions(): Promise<EditSession[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/sessions'),
	);
	const rows = Array.isArray(data)
		? (data as EditSessionDto[])
		: extractPage<EditSessionDto>(data).items;
	return rows.map(toEditSession);
}

/** Begin a new edit session with an optional label. */
export async function beginEditSession(label?: string): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/file-checkpoint/sessions', {
			body: { label: label ?? null },
		}),
	);
	return String(requireData(data, 'Edit session'));
}

/** Roll back an entire edit session on an actor partition. */
export async function rollbackEditSession(
	sessionId: string,
	actor: string,
): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}', {
			params: { path: { id: sessionId, actor } },
		}),
	);
	return String(requireData(data, 'Session rollback'));
}

/** Undo the last edit on an actor partition. */
export async function undoEdit(actor: string): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/file-checkpoint/undo/{id}', {
			params: { path: { id: actor } },
		}),
	);
	return String(requireData(data, 'Undo'));
}

/** Redo the most recently undone edit on an actor partition. */
export async function redoEdit(actor: string): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/file-checkpoint/redo/{id}', {
			params: { path: { id: actor } },
		}),
	);
	return String(requireData(data, 'Redo'));
}

export interface WorkspaceFile {
	path: string;
	hash: string;
	timestamp: number;
}

/** Reconstructed file set of an actor partition (metadata only). */
export async function getActorWorkspace(
	actor: string,
): Promise<WorkspaceFile[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/workspace/{id}', {
			params: { path: { id: actor } },
		}),
	);
	const rows = Array.isArray(data)
		? (data as Array<{ path?: string; hash?: string; timestamp?: number }>)
		: extractPage<{ path?: string; hash?: string; timestamp?: number }>(data)
				.items;
	return rows.map((f) => ({
		path: f.path ?? '',
		hash: f.hash ?? '',
		timestamp: f.timestamp ?? 0,
	}));
}

export interface IndexStatus {
	indexedCommits: number;
	totalCommits: number;
	needsRebuild: boolean;
}

/** Source-index health: indexed rows versus reachable commits. */
export async function getIndexStatus(): Promise<IndexStatus> {
	const data = await call<{
		indexed_commits?: number;
		total_commits?: number;
		needs_rebuild?: boolean;
	}>(client.GET('/api/v1/file-checkpoint/index/status'));
	const view = requireData(data, 'Index status');
	return {
		indexedCommits: view.indexed_commits ?? 0,
		totalCommits: view.total_commits ?? 0,
		needsRebuild: view.needs_rebuild ?? false,
	};
}

/** Rebuild the source index from the commit graph. */
export async function rebuildIndex(): Promise<{ indexedCommits: number }> {
	const data = await call<{ indexed_commits?: number }>(
		client.POST('/api/v1/file-checkpoint/index/rebuild'),
	);
	const view = requireData(data, 'Index rebuild');
	return { indexedCommits: view.indexed_commits ?? 0 };
}

/** Create a full workspace file checkpoint for an actor. */
export async function createFileCheckpoint(actor: string): Promise<{
	id: string;
	timestamp: number;
	fileCount: number;
}> {
	const data = await call<{
		id?: string;
		timestamp?: number;
		file_count?: number;
	}>(
		client.POST('/api/v1/file-checkpoint/checkpoints', {
			body: { actor },
		}),
	);
	const view = requireData(data, 'File checkpoint');
	return {
		id: view.id ?? '',
		timestamp: view.timestamp ?? 0,
		fileCount: view.file_count ?? 0,
	};
}

/** Restore the bound workspace from a file checkpoint. */
export async function restoreFileCheckpoint(
	entity: string,
	checkpoint: string,
): Promise<{ restored: number }> {
	const data = await call<{ restored?: number }>(
		client.POST('/api/v1/file-checkpoint/restore', {
			body: { entity, checkpoint },
		}),
	);
	const view = requireData(data, 'File restore');
	return { restored: view.restored ?? 0 };
}

/** Submit an actor edit line for review (edit ref copy, no merge). */
export async function submitForReview(
	actor: string,
	session?: string,
): Promise<{ reviewCommit: string }> {
	const data = await call<{ review_commit?: string }>(
		client.POST('/api/v1/file-checkpoint/submit', {
			body: { actor, session: session ?? null },
		}),
	);
	const view = requireData(data, 'Review submit');
	return { reviewCommit: view.review_commit ?? '' };
}

export interface MergeToFeatureOutcome {
	commitId: string;
	conflictFiles: string[];
}

interface MergeToFeatureDto {
	commit_id?: string;
	commitId?: string;
	conflict_files?: string[];
	conflictFiles?: string[];
}

/** Merge an actor submission into a feature ref via three-way merge. */
export async function mergeToFeature(
	actor: string,
	feature: string,
	session?: string,
): Promise<MergeToFeatureOutcome> {
	const data = await call<MergeToFeatureDto>(
		client.POST('/api/v1/file-checkpoint/merge-to-feature', {
			body: { actor, feature, session: session ?? null },
		}),
	);
	const view = requireData(data, 'Merge to feature');
	return {
		commitId: view.commit_id ?? view.commitId ?? '',
		conflictFiles: view.conflict_files ?? view.conflictFiles ?? [],
	};
}

export interface MergeToMainOutcome {
	checkpointId: string;
	commitId: string;
	conflictFiles: string[];
}

interface MergeToMainDto {
	checkpoint_id?: string;
	checkpointId?: string;
	merge_result?: MergeToFeatureDto | null;
	mergeResult?: MergeToFeatureDto | null;
}

/** Merge named features into main in input order, one merge commit per feature. */
export async function mergeToMain(
	actor: string,
	features: string[],
	session?: string,
): Promise<MergeToMainOutcome> {
	const data = await call<MergeToMainDto>(
		client.POST('/api/v1/file-checkpoint/merge-to-main', {
			body: { actor, features, session: session ?? null },
		}),
	);
	const view = requireData(data, 'Merge to main');
	const nested = view.merge_result ?? view.mergeResult ?? null;
	return {
		checkpointId: view.checkpoint_id ?? view.checkpointId ?? '',
		commitId: nested?.commit_id ?? nested?.commitId ?? '',
		conflictFiles: nested?.conflict_files ?? nested?.conflictFiles ?? [],
	};
}

/** Roll back one merged feature from main by appending a revert commit. */
export async function rollbackFeature(
	actor: string,
	featureHead: string,
	session?: string,
): Promise<{ revertCommit: string }> {
	const data = await call<{ revert_commit?: string }>(
		client.POST('/api/v1/file-checkpoint/rollback-feature', {
			body: { actor, feature_head: featureHead, session: session ?? null },
		}),
	);
	const view = requireData(data, 'Feature rollback');
	return { revertCommit: view.revert_commit ?? '' };
}
