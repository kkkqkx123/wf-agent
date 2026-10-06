import { client } from '$lib/api/client';
import { call } from '$lib/api/envelope';
import type { components } from '$lib/api/schema';

type LockView = components['schemas']['LockView'];

export interface WorkflowLock {
	ownerId: string;
	ownerName: string;
	expiresAt: number;
}

function toLock(value: LockView | null): WorkflowLock | null {
	if (!value) return null;
	if (typeof value.owner_id !== 'string' || !value.owner_id) return null;
	return {
		ownerId: value.owner_id,
		ownerName: value.owner_name,
		expiresAt: value.expires_at,
	};
}

/** Current lock holder; null means unlocked. */
export async function getWorkflowLock(
	workflowId: string,
): Promise<WorkflowLock | null> {
	const data = await call<LockView | null>(
		client.GET('/api/v1/workflows/{id}/lock', {
			params: { path: { id: workflowId } },
		}),
	);
	if (data === null) return null;
	return toLock(data);
}

/** Acquire the lock for an owner; returns the active holder. */
export async function acquireWorkflowLock(
	workflowId: string,
	owner: { ownerId: string; ownerName: string },
): Promise<WorkflowLock> {
	const data = await call<LockView | null>(
		client.POST('/api/v1/workflows/{id}/lock/acquire', {
			params: { path: { id: workflowId } },
			body: { owner_id: owner.ownerId, owner_name: owner.ownerName },
		}),
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
	const data = await call<LockView | null>(
		client.POST('/api/v1/workflows/{id}/lock/heartbeat', {
			params: { path: { id: workflowId } },
			body: { owner_id: ownerId },
		}),
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
	await call<LockView | null>(
		client.POST('/api/v1/workflows/{id}/lock/release', {
			params: { path: { id: workflowId } },
			body: { owner_id: ownerId },
		}),
	);
}
