import { client, request } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
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
		restorable: d.restorable ?? String(d.status ?? '').toLowerCase() !== 'failed',
		chainPosition: d.chain_position ?? d.chainPosition ?? sequence,
		status: d.status ?? 'completed',
		tags: d.tags ?? [],
		entityType: d.entity_type ?? d.entityType ?? ''
	};
}

/** List checkpoints with optional paging. */
export async function listCheckpoints(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Checkpoint>> {
	const data = await call<unknown>(
		request('GET', '/api/v1/checkpoints', {
			params: { query: { limit: params?.limit, offset: params?.offset } }
		})
	);
	const page = extractPage<CheckpointDto>(data);
	const rows = page.items.length > 0 ? page.items : Array.isArray(data) ? (data as CheckpointDto[]) : [];
	return {
		...page,
		items: rows.map((d, index) => toCheckpoint(d, index))
	};
}

/** Checkpoints for one agent loop, newest first. */
export async function listLoopCheckpoints(loopId: string): Promise<Checkpoint[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/checkpoints/chain', {
			params: { path: { id: loopId } }
		})
	);
	const page = extractPage<CheckpointDto>(data);
	const rows = page.items.length > 0 ? page.items : Array.isArray(data) ? (data as CheckpointDto[]) : [];
	return rows.map((d, index) => toCheckpoint(d, index));
}

/** Checkpoint statistics. */
export async function getCheckpointStats(): Promise<object> {
	const data = await call<object>(client.GET('/api/v1/agent-checkpoints/stats'));
	return data ?? {};
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
	const raw = String(d.change_type ?? d.changeType ?? d.kind ?? '').toLowerCase();
	const changeType = (
		raw === 'added' || raw === 'modified' || raw === 'deleted' || raw === 'renamed' ? raw : 'modified'
	) as FileChange['changeType'];
	return {
		id: d.id ?? `fc-${index}`,
		path: d.path ?? d.file ?? '',
		changeType,
		actor: d.actor ?? d.actor_id ?? '',
		at: typeof d.at === 'string' ? d.at : toIso(d.timestamp),
		additions: d.additions ?? 0,
		deletions: d.deletions ?? 0,
		session: d.session ?? d.session_id ?? ''
	};
}

/** File changes for an execution or checkpoint. */
export async function getFileChanges(executionId?: string): Promise<FileChange[]> {
	const data = await call<unknown>(
		request('GET', '/api/v1/file-checkpoint/changes', {
			params: { query: {} }
		})
	);
	const page = extractPage<FileChangeDto>(data);
	const rows = page.items.length > 0 ? page.items : Array.isArray(data) ? (data as FileChangeDto[]) : [];
	return rows.map((d, index) => toFileChange(d, index));
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
export async function getApprovalRequests(status?: string): Promise<Approval[]> {
	const data = await call<unknown>(client.GET('/api/v1/file-checkpoint/approvals/pending'));
	const page = extractPage<ApprovalDto>(data);
	const rows = page.items.length > 0 ? page.items : Array.isArray(data) ? (data as ApprovalDto[]) : [];
	return rows
		.map((d, index) => ({
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
			executionId: d.execution_id ?? d.executionId ?? ''
		}) satisfies Approval)
		.filter((a) => !status || a.status === status);
}

/** Restore from a checkpoint. */
export async function restoreFromCheckpoint(checkpointId: string, loopId?: string): Promise<boolean> {
	if (loopId) {
		await call<unknown>(
			request('POST', '/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', {
				params: { path: { id: loopId, cid: checkpointId } }
			})
		);
		return true;
	}
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/restore', {
			params: { path: { cid: checkpointId } }
		})
	);
	return true;
}

/** Resume execution from a checkpoint. */
export async function resumeFromCheckpoint(checkpointId: string): Promise<boolean> {
	await call<unknown>(
		request('POST', '/api/v1/executions/checkpoints/{cid}/resume', {
			params: { path: { cid: checkpointId } }
		})
	);
	return true;
}
