import { request } from '$lib/api/client';
import { call } from '$lib/api/envelope';

export interface WorkflowLock {
	ownerId: string;
	ownerName: string;
	expiresAt: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function toLock(value: unknown): WorkflowLock | null {
	if (!isRecord(value)) return null;
	const ownerId = value.owner_id ?? value.ownerId;
	const ownerName = value.owner_name ?? value.ownerName ?? '';
	const expiresAt = value.expires_at ?? value.expiresAt ?? 0;
	if (typeof ownerId !== 'string' || !ownerId) return null;
	return {
		ownerId,
		ownerName: typeof ownerName === 'string' ? ownerName : '',
		expiresAt: typeof expiresAt === 'number' ? expiresAt : 0,
	};
}

/** Current lock holder; null means unlocked. */
export async function getWorkflowLock(
	workflowId: string,
): Promise<WorkflowLock | null> {
	const data = await call<unknown>(
		request(
			'GET',
			`/api/v1/workflows/${encodeURIComponent(workflowId)}/lock`,
		),
	);
	if (data === null) return null;
	return toLock(data);
}

/** Acquire the lock for an owner; returns the active holder. */
export async function acquireWorkflowLock(
	workflowId: string,
	owner: { ownerId: string; ownerName: string },
): Promise<WorkflowLock> {
	const data = await call<unknown>(
		request(
			'POST',
			`/api/v1/workflows/${encodeURIComponent(workflowId)}/lock/acquire`,
			{ body: { owner_id: owner.ownerId, owner_name: owner.ownerName } },
		),
	);
	const lock = toLock(data);
	if (!lock) throw new Error('Lock acquisition returned no holder');
	return lock;
}

/** Renew the lease while editing. */
export async function heartbeatWorkflowLock(
	workflowId: string,
	ownerId: string,
): Promise<WorkflowLock> {
	const data = await call<unknown>(
		request(
			'POST',
			`/api/v1/workflows/${encodeURIComponent(workflowId)}/lock/heartbeat`,
			{ body: { owner_id: ownerId } },
		),
	);
	const lock = toLock(data);
	if (!lock) throw new Error('Lock heartbeat returned no holder');
	return lock;
}

/** Release the lock; no-op when held by someone else. */
export async function releaseWorkflowLock(
	workflowId: string,
	ownerId: string,
): Promise<void> {
	await call<unknown>(
		request(
			'POST',
			`/api/v1/workflows/${encodeURIComponent(workflowId)}/lock/release`,
			{ body: { owner_id: ownerId } },
		),
	);
}
