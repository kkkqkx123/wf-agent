import { client, request } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	Checkpoint,
	Approval,
	FileActor,
	FileChange,
	EditSession,
} from '$lib/types/models';

interface CheckpointDto {
	id?: string;
	entity_type?: string;
	entity_id?: string;
	checkpoint_type?: string;
	timestamp?: number;
	status?: string;
	previous_checkpoint_id?: string | null;
	base_checkpoint_id?: string | null;
	chain_root_id?: string | null;
	chain_position?: number | null;
	blob_size?: number | null;
	tags?: string[] | null;
}

function toCheckpoint(d: CheckpointDto): Checkpoint {
	return {
		id: d.id ?? '',
		entityId: d.entity_id ?? '',
		entityType: d.entity_type ?? '',
		kind: d.checkpoint_type ?? '',
		status: d.status ?? '',
		createdAt: d.timestamp ? new Date(d.timestamp).toISOString() : '',
		sizeBytes: d.blob_size ?? null,
		chainPosition: d.chain_position ?? null,
		chainRootId: d.chain_root_id ?? null,
		tags: d.tags ?? [],
	};
}

export async function listCheckpoints(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const data = await call<unknown>(
		request('GET', '/api/v1/checkpoints', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<CheckpointDto>(data);
	return { ...page, items: page.items.map(toCheckpoint) };
}

/**
 * Checkpoints of one agent loop. utoipa shares the
 * `handle_list_checkpoints` operation name across routes, so the untyped
 * `request()` helper carries the path parameter instead.
 */
export async function listLoopCheckpoints(
	entityId: string,
): Promise<Checkpoint[]> {
	const data = await call<unknown>(
		request('GET', '/api/v1/agent-loops/{id}/checkpoints', {
			params: { path: { id: entityId }, query: { limit: 100 } },
		}),
	);
	if (Array.isArray(data)) {
		return (data as CheckpointDto[]).map(toCheckpoint);
	}
	return extractPage<CheckpointDto>(data).items.map(toCheckpoint);
}

interface PendingApprovalDto {
	actor?: string;
	snapshot_id?: string;
	submitted_at?: number;
	changes?: { file?: string }[];
}

function toApproval(d: PendingApprovalDto): Approval {
	const files = (d.changes ?? [])
		.map((change) => change.file ?? '')
		.filter(Boolean);
	return {
		id: d.actor ?? '',
		title: `Pending file changes · ${files.length} file${files.length === 1 ? '' : 's'}`,
		kind: 'file-approval',
		requester: d.actor ?? '',
		requestedAt: d.submitted_at ? new Date(d.submitted_at).toISOString() : '',
		status: 'pending',
		detail: files.slice(0, 3).join(', ') + (files.length > 3 ? ' …' : ''),
		executionId: d.snapshot_id ?? '',
	};
}

export async function listPendingApprovals(): Promise<Approval[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/approvals/pending', {}),
	);
	if (Array.isArray(data)) {
		return (data as PendingApprovalDto[]).map(toApproval);
	}
	return [];
}

export async function approveChanges(actorId: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/file-checkpoint/approvals/{id}/approve', {
			params: { path: { id: actorId } },
			body: { feature: '', paths: null },
		}),
	);
}

export async function rejectChanges(
	actorId: string,
	reason?: string,
): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/file-checkpoint/approvals/{id}/reject', {
			params: { path: { id: actorId } },
			body: { reason: reason ?? null },
		}),
	);
}

/**
 * Restore an execution checkpoint. utoipa emits two handlers with this name
 * (agent + workflow domains), so the generated param types do not describe this
 * route; the untyped `request()` helper carries the path parameter instead.
 */
export async function restoreCheckpoint(checkpointId: string): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/restore', {
			params: { path: { cid: checkpointId } },
		}),
	);
}

export async function resumeFromCheckpoint(
	checkpointId: string,
): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/resume', {
			params: { path: { cid: checkpointId } },
		}),
	);
}

export async function deleteCheckpoint(checkpointId: string): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/checkpoints/{id}', {
			params: { path: { id: checkpointId } },
		}),
	);
}

interface PartitionDto {
	partition_id?: string;
	kind?: string;
	actor?: string;
	history_len?: number;
}

/** Actor partitions of the file-checkpoint store, which the workspace reads key on. */
export async function listFileActors(): Promise<FileActor[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/partitions', {}),
	);
	const rows = (Array.isArray(data) ? data : []) as PartitionDto[];
	return rows
		.filter((row): row is PartitionDto & { actor: string } =>
			Boolean(row.actor),
		)
		.map((row) => ({
			actor: row.actor,
			kind: row.kind ?? 'unknown',
			historyLen: row.history_len ?? 0,
		}));
}

interface FileDiffDto {
	path?: string;
	kind?: string;
	diff?: string;
	additions?: number;
	deletions?: number;
}

/** Per-file difference between an actor workspace and the staged partition. */
export async function listStagedChanges(actor: string): Promise<FileChange[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/diff/staged/{id}', {
			params: { path: { id: actor } },
		}),
	);
	const rows = (Array.isArray(data) ? data : []) as FileDiffDto[];
	return rows
		.filter((row) => row.path && row.kind && row.kind !== 'unchanged')
		.map((row) => ({
			path: row.path as string,
			kind:
				row.kind === 'added' || row.kind === 'deleted' ? row.kind : 'modified',
			additions: row.additions ?? 0,
			deletions: row.deletions ?? 0,
			diff: row.diff ?? null,
		}));
}

interface EditSessionDto {
	id?: string | number;
	label?: string | null;
	created_at?: number;
	snapshot_ids?: unknown[];
	delta_ids?: unknown[];
}

/** Persisted edit sessions, newest first. */
export async function listEditSessions(): Promise<EditSession[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/file-checkpoint/sessions', {
			params: { query: { limit: 50 } },
		}),
	);
	const rows = (
		Array.isArray(data) ? data : extractPage<EditSessionDto>(data).items
	) as EditSessionDto[];
	return rows
		.filter((row) => row.id !== undefined)
		.map((row) => ({
			id: String(row.id),
			label: row.label || '(unlabeled)',
			createdAt: row.created_at ? new Date(row.created_at).toISOString() : '',
			changeCount:
				(row.snapshot_ids?.length ?? 0) + (row.delta_ids?.length ?? 0),
		}));
}

/** Undo the last edit recorded on an actor partition. */
export async function undoEdit(actor: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/file-checkpoint/undo/{id}', {
			params: { path: { id: actor } },
		}),
	);
}

/** Redo the most recently undone edit on an actor partition. */
export async function redoEdit(actor: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/file-checkpoint/redo/{id}', {
			params: { path: { id: actor } },
		}),
	);
}

export async function rollbackSession(
	sessionId: string,
	actor: string,
): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}', {
			params: { path: { id: sessionId, actor } },
		}),
	);
}
