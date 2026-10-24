import { client } from '#lib/api/client';
import { call, extractPage, requireData } from '#lib/api/envelope';
import type { PageResult } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';
import type {
	Approval,
	Checkpoint,
	FileChange,
	FileChangeType,
} from '#lib/types/models';
import { toDiffFile } from '#lib/types/diff';
import type { DiffFile } from '#lib/types/diff';

// Transport row types come from the generated OpenAPI snapshot; only the
// page-facing view models in `#lib/types/models` are hand-maintained.
type CheckpointDoc = components['schemas']['CheckpointDoc'];
type DeltaSummaryDoc = components['schemas']['DeltaSummaryDoc'];
type PendingApprovalDoc = components['schemas']['PendingApprovalDoc'];
type FileDiffDoc = components['schemas']['FileDiffDoc'];
type FileContentDoc = components['schemas']['FileContentDoc'];
type FileTreeDoc = components['schemas']['FileTreeDoc'];
type FileTimelineDoc = components['schemas']['FileTimelineDoc'];
type PartitionDoc = components['schemas']['PartitionDoc'];
type ConflictFileDoc = components['schemas']['ConflictFileDoc'];
type EditGroupDoc = components['schemas']['EditGroupDoc'];
type WorkspaceFileDoc = components['schemas']['WorkspaceFileDoc'];
type IndexStatusDoc = components['schemas']['IndexStatusDoc'];
type FileCheckpointSummaryDoc =
	components['schemas']['FileCheckpointSummaryDoc'];
type GitMergeOutcomeDoc = components['schemas']['GitMergeOutcomeDoc'];
type MergeCommitDoc = components['schemas']['MergeCommitDoc'];
type GcStatsDoc = components['schemas']['GcStatsDoc'];
type StoreStatsDoc = components['schemas']['StoreStatsDoc'];
type StateFileLinkDoc = components['schemas']['StateFileLinkDoc'];
type PartitionHistoryEntryDoc =
	components['schemas']['PartitionHistoryEntryDoc'];
type EntityMetadataDoc = components['schemas']['EntityMetadataDoc'];

type CheckpointPage = NonNullable<
	components['schemas']['ApiEnvelope_PageView_CheckpointDoc']['data']
>;
type ChangesPage = NonNullable<
	components['schemas']['ApiEnvelope_PageView_DeltaSummaryDoc']['data']
>;
type SessionsPage = NonNullable<
	components['schemas']['ApiEnvelope_PageView_EditGroupDoc']['data']
>;

function toIso(value: number | string | null | undefined): string {
	if (typeof value === 'string') return value;
	if (typeof value === 'number') return new Date(value).toISOString();
	return '';
}

function toCheckpoint(d: CheckpointDoc, index: number): Checkpoint {
	const sequence = d.chain_position ?? index;
	return {
		id: d.id,
		executionId: d.entity_id,
		sequence,
		kind: d.checkpoint_type,
		actor: '',
		createdAt: toIso(d.timestamp),
		sizeBytes: d.blob_size ?? 0,
		note: '',
		restorable: d.status.toLowerCase() !== 'failed',
		chainPosition: d.chain_position ?? sequence,
		status: d.status,
		tags: d.tags ?? [],
		entityType: d.entity_type,
	};
}

/** List checkpoints with optional paging. */
export async function listCheckpoints(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const data = await call<CheckpointPage | CheckpointDoc[]>(
		client.GET('/api/v1/checkpoints', {
			params: { query: { limit: params?.limit, offset: params?.offset } },
		}),
	);
	const page = extractPage<CheckpointDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as CheckpointDoc[])
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
	const data = await call<{ items: CheckpointDoc[] } | CheckpointDoc[]>(
		client.GET('/api/v1/agent-loops/{id}/checkpoints/chain', {
			params: { path: { id: loopId } },
		}),
	);
	const page = extractPage<CheckpointDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as CheckpointDoc[])
				: [];
	return rows.map((d, index) => toCheckpoint(d, index));
}

/** Narrow a wire change kind onto the view-model vocabulary. */
function toFileChangeType(kind: string): FileChangeType | null {
	switch (kind) {
		case 'added':
		case 'modified':
		case 'deleted':
		case 'renamed':
			return kind;
		default:
			return null;
	}
}

function toFileChange(d: DeltaSummaryDoc, index: number): FileChange {
	// DeltaSummary reports the change relative to the parent commit: kind plus
	// added/removed line counts. `source` is the origin label (actor id /
	// `human` / `merge` / `review`) and doubles as the actor owning the change;
	// `snapshot_id` scopes the row id to its commit.
	return {
		id: d.snapshot_id ? `${d.snapshot_id}:${d.file}` : `fc-${index}`,
		path: d.file,
		changeType: toFileChangeType(d.kind),
		actor: d.source ?? '',
		at: toIso(d.timestamp),
		additions: d.additions,
		deletions: d.deletions,
		session: '',
	};
}

/** Paged file changes with cursor state. */
export async function getFileChangesPage(params?: {
	actor?: string;
	path?: string;
	limit?: number;
	offset?: number;
}): Promise<PageResult<FileChange>> {
	const data = await call<ChangesPage | DeltaSummaryDoc[]>(
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
	const page = extractPage<DeltaSummaryDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as DeltaSummaryDoc[])
				: [];
	return {
		...page,
		items: rows.map((d, index) => toFileChange(d, index)),
	};
}

function toApproval(d: PendingApprovalDoc, index: number): Approval {
	// The wire row carries actor + snapshot + the submitted file set; the
	// titled view fields are derived from that file set so the card is not
	// blank. `{id}` on approve/reject is the actor partition id, and every
	// listed item is pending by definition.
	const files = d.changes.map((change) => change.file);
	const actor = d.actor;
	const executionId = actor.startsWith('agent:')
		? actor.slice('agent:'.length)
		: actor;
	const detail =
		files.length === 0
			? 'No file rows recorded on the submitted snapshot.'
			: files.length > 8
				? `${files.slice(0, 8).join(', ')} … +${files.length - 8} more`
				: files.join(', ');
	return {
		id: actor || `approval-${index}`,
		title:
			files.length === 0
				? 'Pending file changes'
				: `${files.length} file(s) pending review`,
		kind: 'file review',
		requester: actor,
		requestedAt: toIso(d.submitted_at),
		status: 'pending',
		detail,
		executionId,
	};
}

/** Pending approval requests. */
export async function getApprovalRequests(): Promise<Approval[]> {
	const data = await call<PendingApprovalDoc[]>(
		client.GET('/api/v1/file-checkpoint/approvals/pending'),
	);
	return data.map((d, index) => toApproval(d, index));
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
	await call<boolean>(
		client.POST('/api/v1/executions/checkpoints/{cid}/restore', {
			params: { path: { cid: checkpointId } },
		}),
	);
	return true;
}

export interface RestorePreview {
	checkpointId: string;
	executionId: string;
	status: string;
	currentNodeId: string | null;
	nodeResultKeys: string[];
	variableKeys: string[];
}

/** Preview what a restore would bring back, without side effects. */
export async function previewRestoreCheckpoint(
	checkpointId: string,
): Promise<RestorePreview> {
	return call<RestorePreview>(
		client.GET('/api/v1/executions/checkpoints/{cid}/preview', {
			params: { path: { cid: checkpointId } },
		}),
	);
}

/** Restore a listed checkpoint through the endpoint matching its entity. */
export async function restoreCheckpoint(target: Checkpoint): Promise<boolean> {
	const loopId =
		target.entityType === 'agent_loop' ? target.executionId : undefined;
	return restoreFromCheckpoint(target.id, loopId || undefined);
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

/** Per-file diffs of one actor workspace against main, backing the diff preview. */
export async function getMainDiffs(actor: string): Promise<DiffFile[]> {
	const data = await call<FileDiffDoc[]>(
		client.GET('/api/v1/file-checkpoint/diff/main/{id}', {
			params: { path: { id: actor } },
		}),
	);
	return data.map(toDiffFile);
}

/** Per-file diff between two actor workspaces. */
export async function getDiffActors(a: string, b: string): Promise<DiffFile[]> {
	const data = await call<FileDiffDoc[]>(
		client.GET('/api/v1/file-checkpoint/diff/actors/{a}/{b}', {
			params: { path: { a, b } },
		}),
	);
	return data.map(toDiffFile);
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

/** Read-only file content for one actor workspace, optionally pinned to
 * a historical commit (`version` is always a commit id). Versioned reads
 * do not touch the workspace, so conflicted bytes can be read at the
 * conflicted merge commit for in-place resolution. */
export async function getFileContent(
	actor: string,
	path: string,
	version?: string,
): Promise<FileContent> {
	const d = requireData(
		await call<FileContentDoc>(
			client.GET('/api/v1/file-checkpoint/content', {
				params: { query: { actor, path, version } },
			}),
		),
		`File content missing for ${path}`,
	);
	return {
		path: d.path,
		actor: d.actor,
		hash: d.hash,
		size: d.size,
		isBinary: d.is_binary,
		content: d.content ?? null,
		truncated: d.truncated,
		timestamp: d.timestamp,
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

/** Capped directory tree for one actor workspace. */
export async function getFileTree(
	actor: string,
	prefix?: string,
): Promise<FileTree> {
	const d = requireData(
		await call<FileTreeDoc>(
			client.GET('/api/v1/file-checkpoint/tree/{id}', {
				params: { path: { id: actor }, query: { prefix } },
			}),
		),
		`File tree missing for ${actor}`,
	);
	return {
		entries: d.entries,
		truncated: d.truncated,
		total: d.total,
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

/** Version timeline for one file path, including rename history. */
export async function getFileTimeline(path: string): Promise<FileTimeline> {
	const d = requireData(
		await call<FileTimelineDoc>(
			client.GET('/api/v1/file-checkpoint/timeline/{id}', {
				params: { path: { id: path } },
			}),
		),
		`File timeline missing for ${path}`,
	);
	return {
		originalPath: d.original_path,
		entries: d.entries.map((entry) => ({
			path: entry.path,
			snapshotId: entry.snapshot_id,
			contentHash: entry.content_hash,
			timestamp: entry.timestamp,
			source: entry.source,
			movedFrom: entry.moved_from ?? null,
		})),
		truncated: d.truncated,
		total: d.total,
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

/** Replace the tags on a checkpoint. */
export async function setCheckpointTags(
	id: string,
	tags: string[],
): Promise<void> {
	await call<unknown>(
		client.PUT('/api/v1/checkpoints/{id}/tags', {
			params: { path: { id } },
			body: tags,
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

/** All partitions of the file-checkpoint store. */
export async function listPartitions(): Promise<PartitionView[]> {
	const rows = await call<PartitionDoc[]>(
		client.GET('/api/v1/file-checkpoint/partitions'),
	);
	return rows.map((d) => ({
		partitionId: d.partition_id,
		name: d.name,
		kind: d.kind,
		actor: d.actor ?? null,
		currentSnapshot: d.current_snapshot,
		historyLen: d.history_len,
		createdAt: d.created_at,
		updatedAt: d.updated_at,
	}));
}

export interface ConflictRegion {
	file: string;
	startLine: number;
	base: string[];
	ours: string[];
	theirs: string[];
}

export interface ConflictFile {
	path: string;
	snapshotId: string;
	partition: string;
	conflictCount: number;
	regions: ConflictRegion[];
}

/** Files with unresolved merge conflicts, enumerated from unresolved merge commits. */
export async function listConflicts(): Promise<ConflictFile[]> {
	const rows = await call<ConflictFileDoc[]>(
		client.GET('/api/v1/file-checkpoint/conflicts'),
	);
	return rows.map((d) => ({
		path: d.path,
		snapshotId: d.snapshot_id,
		partition: d.partition,
		conflictCount: d.conflicts.length,
		regions: d.conflicts.map((region) => ({
			file: region.file,
			startLine: region.start_line,
			base: region.base,
			ours: region.ours,
			theirs: region.theirs,
		})),
	}));
}

/** Resolve conflicts on a feature ref with full-file resolved contents. */
export async function resolveConflicts(
	actor: string,
	feature: string,
	resolutions: Record<string, string>,
	session?: string,
): Promise<{ remaining: number }> {
	const d = requireData(
		await call<components['schemas']['ResolveConflictsResponse']>(
			client.POST('/api/v1/file-checkpoint/conflicts/resolve', {
				body: { actor, feature, resolutions, session: session ?? null },
			}),
		),
		'Conflict resolution',
	);
	return { remaining: d.remaining };
}

export interface EditSession {
	id: string;
	label: string | null;
	createdAt: number;
}

/** Staged edit sessions (newest first). */
export async function listEditSessions(): Promise<EditSession[]> {
	const data = await call<SessionsPage | EditGroupDoc[]>(
		client.GET('/api/v1/file-checkpoint/sessions'),
	);
	const page = extractPage<EditGroupDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as EditGroupDoc[])
				: [];
	return rows.map((d) => ({
		id: d.id,
		label: d.label ?? null,
		createdAt: d.created_at,
	}));
}

/** Begin a new edit session with an optional label. */
export async function beginEditSession(label?: string): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/file-checkpoint/sessions', {
				body: { label: label ?? null },
			}),
		),
		'Edit session',
	);
}

/** Roll back an entire edit session on an actor partition. */
export async function rollbackEditSession(
	sessionId: string,
	actor: string,
): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}', {
				params: { path: { id: sessionId, actor } },
			}),
		),
		'Session rollback',
	);
}

/** Undo the last edit on an actor partition. */
export async function undoEdit(actor: string): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/file-checkpoint/undo/{id}', {
				params: { path: { id: actor } },
			}),
		),
		'Undo',
	);
}

/** Redo the most recently undone edit on an actor partition. */
export async function redoEdit(actor: string): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/file-checkpoint/redo/{id}', {
				params: { path: { id: actor } },
			}),
		),
		'Redo',
	);
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
	const rows = await call<WorkspaceFileDoc[]>(
		client.GET('/api/v1/file-checkpoint/workspace/{id}', {
			params: { path: { id: actor } },
		}),
	);
	return rows.map((f) => ({
		path: f.path,
		hash: f.hash,
		timestamp: f.timestamp,
	}));
}

export interface IndexStatus {
	indexedCommits: number;
	totalCommits: number;
	needsRebuild: boolean;
}

/** Source-index health: indexed rows versus reachable commits. */
export async function getIndexStatus(): Promise<IndexStatus> {
	const d = requireData(
		await call<IndexStatusDoc>(
			client.GET('/api/v1/file-checkpoint/index/status'),
		),
		'Index status',
	);
	return {
		indexedCommits: d.indexed_commits,
		totalCommits: d.total_commits,
		needsRebuild: d.needs_rebuild,
	};
}

/** Rebuild the source index from the commit graph. */
export async function rebuildIndex(): Promise<{ indexedCommits: number }> {
	const d = requireData(
		await call<components['schemas']['RebuildIndexResponse']>(
			client.POST('/api/v1/file-checkpoint/index/rebuild'),
		),
		'Index rebuild',
	);
	return { indexedCommits: d.indexed_commits };
}

/** Create a full workspace file checkpoint for an actor. */
export async function createFileCheckpoint(actor: string): Promise<{
	id: string;
	timestamp: number;
	fileCount: number;
}> {
	const d = requireData(
		await call<FileCheckpointSummaryDoc>(
			client.POST('/api/v1/file-checkpoint/checkpoints', {
				body: { actor },
			}),
		),
		'File checkpoint',
	);
	return {
		id: d.id,
		timestamp: d.timestamp,
		fileCount: d.file_count,
	};
}

/** Submit an actor edit line for review (edit ref copy, no merge). */
export async function submitForReview(
	actor: string,
	session?: string,
): Promise<{ reviewCommit: string }> {
	const d = requireData(
		await call<components['schemas']['SubmitReviewResponse']>(
			client.POST('/api/v1/file-checkpoint/submit', {
				body: { actor, session: session ?? null },
			}),
		),
		'Review submit',
	);
	return { reviewCommit: d.review_commit };
}

export interface MergeToFeatureOutcome {
	commitId: string;
	conflictFiles: string[];
}

/** Merge an actor submission into a feature ref via three-way merge. */
export async function mergeToFeature(
	actor: string,
	feature: string,
	session?: string,
): Promise<MergeToFeatureOutcome> {
	const d = requireData(
		await call<GitMergeOutcomeDoc>(
			client.POST('/api/v1/file-checkpoint/merge-to-feature', {
				body: { actor, feature, session: session ?? null },
			}),
		),
		'Merge to feature',
	);
	return {
		commitId: d.commit_id,
		conflictFiles: d.conflict_files,
	};
}

export interface MergeToMainOutcome {
	checkpointId: string;
	commitId: string;
	conflictFiles: string[];
}

/** Merge named features into main in input order, one merge commit per feature. */
export async function mergeToMain(
	actor: string,
	features: string[],
	session?: string,
): Promise<MergeToMainOutcome> {
	const d = requireData(
		await call<MergeCommitDoc>(
			client.POST('/api/v1/file-checkpoint/merge-to-main', {
				body: { actor, features, session: session ?? null },
			}),
		),
		'Merge to main',
	);
	return {
		checkpointId: d.checkpoint_id,
		commitId: d.merge_result.commit_id,
		conflictFiles: d.merge_result.conflict_files,
	};
}

/** Roll back one merged feature from main by appending a revert commit. */
/** Feature rollback result. */
export async function rollbackFeature(
	actor: string,
	featureHead: string,
	session?: string,
): Promise<{ revertCommit: string }> {
	const d = requireData(
		await call<components['schemas']['RollbackFeatureResponse']>(
			client.POST('/api/v1/file-checkpoint/rollback-feature', {
				body: { actor, feature_head: featureHead, session: session ?? null },
			}),
		),
		'Feature rollback',
	);
	return { revertCommit: d.revert_commit };
}

export interface StoreStats {
	commits: number;
	refs: number;
	looseObjects: number;
	storeBytes: number;
	lastGcAt: string | null;
}

/** Read-only object-store statistics for the storage dashboard. */
export async function getStoreStats(): Promise<StoreStats> {
	const d = requireData(
		await call<StoreStatsDoc>(client.GET('/api/v1/file-checkpoint/gc/stats')),
		'Store statistics',
	);
	return {
		commits: d.commits,
		refs: d.refs,
		looseObjects: d.loose_objects,
		storeBytes: d.store_bytes,
		lastGcAt: d.last_gc_ms != null ? toIso(d.last_gc_ms) : null,
	};
}

export interface GcResult {
	removedCheckpoints: number;
	removedSnapshots: number;
	reclaimedSnapshots: number;
}

/** Run a manual GC sweep on the file-checkpoint store. */
export async function runGc(keepRecentHeads = 0): Promise<GcResult> {
	const d = requireData(
		await call<GcStatsDoc>(
			client.POST('/api/v1/file-checkpoint/gc', {
				params: { query: { keep_recent_heads: keepRecentHeads } },
			}),
		),
		'GC run',
	);
	return {
		removedCheckpoints: d.removed_checkpoints,
		removedSnapshots: d.removed_snapshots,
		reclaimedSnapshots: d.reclaimed_snapshots,
	};
}

export type StateFileLinkState = 'linked' | 'pending' | 'missing';

export interface StateFileLink {
	checkpointId: string;
	state: StateFileLinkState;
	fileCommitId: string | null;
}

/** Health of the state-to-file association for a batch of state checkpoints. */
export async function getStateFileLinks(
	checkpointIds: string[],
): Promise<Map<string, StateFileLink>> {
	const ids = checkpointIds.filter((id) => id.length > 0);
	if (ids.length === 0) return new Map();
	const rows = await call<StateFileLinkDoc[]>(
		client.GET('/api/v1/file-checkpoint/state-links', {
			params: { query: { ids: ids.join(',') } },
		}),
	);
	return new Map(
		rows.map((d) => [
			d.checkpoint_id,
			{
				checkpointId: d.checkpoint_id,
				state: d.state as StateFileLinkState,
				fileCommitId: d.file_commit_id ?? null,
			} satisfies StateFileLink,
		]),
	);
}

/** Create a workflow execution checkpoint; returns the new checkpoint id. */
export async function createExecutionCheckpoint(
	executionId: string,
): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/executions/{id}/checkpoints', {
				params: { path: { id: executionId } },
			}),
		),
		'Execution checkpoint',
	);
}

/** Garbage-collect an execution's checkpoints; without `before` all are
 * removed. Returns the number of checkpoints deleted. */
export async function gcExecutionCheckpoints(
	executionId: string,
	before?: number,
): Promise<number> {
	return requireData(
		await call<number>(
			client.POST('/api/v1/executions/{id}/checkpoints/gc', {
				params: { path: { id: executionId }, query: { before } },
			}),
		),
		'Checkpoint GC',
	);
}

/** Delete every checkpoint of one entity; `entityType` selects the domain
 * (`checkpoint` for workflow executions, `agent_loop` for agent loops).
 * Returns the number of checkpoints deleted. */
export async function deleteCheckpointsByEntity(
	entityId: string,
	entityType: string,
): Promise<number> {
	return requireData(
		await call<number>(
			client.DELETE('/api/v1/checkpoints/entity/{entityId}', {
				params: { path: { entityId }, query: { entity_type: entityType } },
			}),
		),
		'Entity checkpoint delete',
	);
}

/** Rename or move a file within an actor workspace. Empty `content` writes
 * an empty destination; pass the raw new bytes to change content too.
 * Returns the snapshot id of the recorded move. */
export async function renameFile(
	actor: string,
	fromPath: string,
	toPath: string,
	content = '',
): Promise<string> {
	return requireData(
		await call<string>(
			client.POST('/api/v1/file-checkpoint/rename', {
				body: { actor, from_path: fromPath, to_path: toPath, content },
			}),
		),
		'File rename',
	);
}

/** One commit in a partition's history, newest first. */
export interface PartitionCommit {
	snapshotId: string;
	parents: string[];
	message: string | null;
	actor: string | null;
	session: string | null;
	tool: string | null;
	timestamp: number;
	isMerge: boolean;
	hasConflicts: boolean;
}

/** Paged commit history of one partition ref. */
export async function getPartitionHistory(
	partitionId: string,
	limit = 50,
	offset = 0,
): Promise<PageResult<PartitionCommit>> {
	const data = await call<
		| NonNullable<
				components['schemas']['ApiEnvelope_PageView_PartitionHistoryEntryDoc']['data']
		  >
		| PartitionHistoryEntryDoc[]
	>(
		client.GET('/api/v1/file-checkpoint/partitions/{id}/history', {
			params: { path: { id: partitionId }, query: { limit, offset } },
		}),
	);
	const page = extractPage<PartitionHistoryEntryDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as PartitionHistoryEntryDoc[])
				: [];
	return {
		...page,
		items: rows.map((d) => ({
			snapshotId: d.snapshot_id,
			parents: d.parents,
			message: d.message ?? null,
			actor: d.actor ?? null,
			session: d.session ?? null,
			tool: d.tool ?? null,
			timestamp: d.timestamp,
			isMerge: d.is_merge,
			hasConflicts: d.has_conflicts,
		})),
	};
}

/** Aggregate statistics of the retained agent-loop checkpoints. */
export interface AgentCheckpointStats {
	total: number;
	byType: Record<string, number>;
	active: number;
	avgBlobSize: number | null;
}

/** Statistics over every retained agent-loop checkpoint. */
export async function getAgentCheckpointStats(): Promise<AgentCheckpointStats> {
	const raw = (await call<unknown>(
		client.GET('/api/v1/agent-checkpoints/stats'),
	)) as {
		total?: number;
		by_type?: Record<string, number>;
		active?: number;
		avg_blob_size?: number | null;
	} | null;
	const data = requireData(raw, 'Agent checkpoint stats');
	return {
		total: data.total ?? 0,
		byType: data.by_type ?? {},
		active: data.active ?? 0,
		avgBlobSize: data.avg_blob_size ?? null,
	};
}

/** Checkpoints of a workflow's executions within an epoch-millis window. */
export async function getCheckpointsByTimeRange(params: {
	workflowId: string;
	start: number;
	end: number;
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const data = await call<CheckpointPage | CheckpointDoc[]>(
		client.GET('/api/v1/checkpoints/time-range', {
			params: {
				query: {
					workflowId: params.workflowId,
					start: params.start,
					end: params.end,
					limit: params.limit,
					offset: params.offset,
				},
			},
		}),
	);
	return toCheckpointPage(data);
}

/** Checkpoints of several entities in one call. `entityType` selects the
 * domain (`checkpoint` for workflow executions, `agent_loop` for loops). */
export async function getCheckpointsByEntities(params: {
	entityIds: string[];
	entityType?: string;
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const ids = params.entityIds.filter((id) => id.trim().length > 0);
	const data = await call<CheckpointPage | CheckpointDoc[]>(
		client.GET('/api/v1/checkpoints/entities', {
			params: {
				query: {
					entity_ids: ids.join(','),
					entity_type: params.entityType,
					limit: params.limit,
					offset: params.offset,
				},
			},
		}),
	);
	return toCheckpointPage(data);
}

/** Share the paged CheckpointDoc → view-model mapping across list queries. */
function toCheckpointPage(
	data: CheckpointPage | CheckpointDoc[] | undefined,
): PageResult<Checkpoint> {
	const page = extractPage<CheckpointDoc>(data);
	const rows =
		page.items.length > 0
			? page.items
			: Array.isArray(data)
				? (data as CheckpointDoc[])
				: [];
	return {
		...page,
		items: rows.map((d, index) => toCheckpoint(d, index)),
	};
}

/** Entity-scoped checkpoint metadata (opaque JSON object). */
export async function getEntityMetadata(
	entityId: string,
): Promise<Record<string, unknown>> {
	const d = requireData(
		await call<EntityMetadataDoc>(
			client.GET('/api/v1/checkpoints/entity/{entityId}/metadata', {
				params: { path: { entityId } },
			}),
		),
		'Entity metadata',
	);
	return (d.metadata ?? {}) as Record<string, unknown>;
}

/** Replace entity-scoped checkpoint metadata with the given JSON object. */
export async function setEntityMetadata(
	entityId: string,
	metadata: Record<string, unknown>,
): Promise<void> {
	await call<unknown>(
		client.PUT('/api/v1/checkpoints/entity/{entityId}/metadata', {
			params: { path: { entityId } },
			body: metadata,
		}),
	);
}

type ExportOutcomeDoc = components['schemas']['ExportOutcomeDoc'];
type ImportOutcomeDoc = components['schemas']['ImportOutcomeDoc'];
type ExternalUpdateHintDoc = components['schemas']['ExternalUpdateHintDoc'];
type ReconcilePointDoc = components['schemas']['ReconcilePointDoc'];
type SyncConfigDoc = components['schemas']['SyncConfigDoc'];
type BaselineRebaseOutcomeDoc =
	components['schemas']['BaselineRebaseOutcomeDoc'];

export interface SyncExportResult {
	suggestedMessage: string;
	externalCommit: string | null;
	internalMain: string;
	idempotent: boolean;
}

export interface SyncImportResult {
	created: string | null;
	advancedTo: string;
	fileCount: number;
	hadConflicts: boolean;
	idempotent: boolean;
}

export interface ExternalHint {
	externalHead: string;
	fileCount: number;
	internalMain: string;
	externalCommit: string;
}

export interface ReconcilePoint {
	internalMain: string;
	externalCommit: string;
	externalRepo: string;
	updatedAtMs: number;
}

/** Export the main tree into the user workspace; call only after explicit confirm. */
export async function exportToWorkspace(
	confirmOverwrite: boolean,
	autoCommit?: boolean,
	message?: string,
): Promise<SyncExportResult> {
	const d = requireData(
		await call<ExportOutcomeDoc>(
			client.POST('/api/v1/file-checkpoint/sync/export', {
				body: {
					confirm_overwrite: confirmOverwrite,
					auto_commit: autoCommit ?? null,
					message: message ?? null,
				},
			}),
		),
		'Sync export',
	);
	return {
		suggestedMessage: d.suggested_message,
		externalCommit: d.external_commit ?? null,
		internalMain: d.reconcile.internal_main,
		idempotent: d.idempotent,
	};
}

/** Import the external head as human history. */
export async function importFromExternal(): Promise<SyncImportResult> {
	const d = requireData(
		await call<ImportOutcomeDoc>(
			client.POST('/api/v1/file-checkpoint/sync/import'),
		),
		'Sync import',
	);
	return {
		created: d.created ?? null,
		advancedTo: d.advanced_to,
		fileCount: d.file_count,
		hadConflicts: d.had_conflicts,
		idempotent: d.idempotent,
	};
}

export interface BaselineSelector {
	mode?: string | null;
	branch?: string | null;
	commitPrefix?: string | null;
	behind?: number | null;
	tag?: string | null;
}

/** Rebase the baseline; call only after explicit confirm. */
export async function rebaseBaseline(
	baseline: BaselineSelector,
): Promise<{ transplantId: string; newAnchor: string }> {
	const d = requireData(
		await call<BaselineRebaseOutcomeDoc>(
			client.POST('/api/v1/file-checkpoint/sync/rebase', {
				body: {
					mode: baseline.mode ?? null,
					branch: baseline.branch ?? null,
					commit_prefix: baseline.commitPrefix ?? null,
					behind: baseline.behind ?? null,
					tag: baseline.tag ?? null,
					confirm: true,
				},
			}),
		),
		'Baseline rebase',
	);
	return {
		transplantId: d.transplant_id,
		newAnchor: d.new_anchor.id,
	};
}

/** External update hint, or null when the external head is aligned. */
export async function getExternalUpdateHint(): Promise<ExternalHint | null> {
	const d = await call<ExternalUpdateHintDoc | null>(
		client.GET('/api/v1/file-checkpoint/sync/hint'),
	);
	if (!d) return null;
	return {
		externalHead: d.external_head,
		fileCount: d.file_count,
		internalMain: d.reconcile.internal_main,
		externalCommit: d.reconcile.external_commit,
	};
}

/** Current reconcile point, or null before the first export. */
export async function getReconcilePoint(): Promise<ReconcilePoint | null> {
	const d = await call<ReconcilePointDoc | null>(
		client.GET('/api/v1/file-checkpoint/sync/reconcile'),
	);
	if (!d) return null;
	return {
		internalMain: d.internal_main,
		externalCommit: d.external_commit,
		externalRepo: d.external_repo,
		updatedAtMs: d.updated_at_ms,
	};
}

export interface SyncDefaults {
	autoCommit: boolean;
	autoImport: boolean;
}

/** Configured sync defaults, or null when unconfigured. */
export async function getSyncConfig(): Promise<SyncDefaults | null> {
	const d = await call<SyncConfigDoc | null>(
		client.GET('/api/v1/file-checkpoint/sync/config'),
	);
	if (!d) return null;
	return {
		autoCommit: d.auto_commit,
		autoImport: d.auto_import,
	};
}
