import { browser } from '$app/environment';
import {
	acquireWorkflowLock,
	getWorkflowLock,
	heartbeatWorkflowLock,
	releaseWorkflowLock,
} from '$lib/services/workflow-locks';

const OWNER_KEY = 'wf-workflow-lock-owner';
const HEARTBEAT_MS = 30_000;
const POLL_MS = 20_000;

function storedOwner(): { ownerId: string; ownerName: string } {
	if (!browser) return { ownerId: '', ownerName: '' };
	try {
		const raw = localStorage.getItem(OWNER_KEY);
		if (raw) {
			const parsed = JSON.parse(raw) as {
				ownerId?: string;
				ownerName?: string;
			};
			if (parsed.ownerId) {
				return {
					ownerId: parsed.ownerId,
					ownerName: parsed.ownerName ?? 'this browser',
				};
			}
		}
	} catch {
		// Corrupt entry falls through to fresh identity.
	}
	const fresh = {
		ownerId: `owner-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
		ownerName: 'this browser',
	};
	try {
		localStorage.setItem(OWNER_KEY, JSON.stringify(fresh));
	} catch {
		// Private mode keeps the in-memory identity only.
	}
	return fresh;
}

/**
 * Single-write lock state for one workflow. The edit store stays unaware
 * of locking; pages gate entering edit, saving and promoting on this
 * store. When the lock endpoints are absent the store reports unsupported
 * and leaves editing unblocked with a notice.
 */
export class WorkflowLockStore {
	holderId = $state<string | null>(null);
	holderName = $state('');
	supported = $state(true);
	refreshError = $state<string | null>(null);

	private workflowId = $state('');
	private ownerId = $state('');
	private ownerName = $state('');
	private heartbeatTimer: number | null = null;
	private pollTimer: number | null = null;

	get held(): boolean {
		return (
			this.supported &&
			this.holderId !== null &&
			this.ownerId !== '' &&
			this.holderId === this.ownerId
		);
	}

	get lockedByOther(): boolean {
		return (
			this.supported &&
			this.holderId !== null &&
			this.holderId !== this.ownerId
		);
	}

	get canWrite(): boolean {
		return !this.supported || this.held;
	}

	get displayHolder(): string {
		if (!this.supported) return 'No lock protection';
		if (!this.holderId) return 'Unlocked';
		if (this.held) return 'You hold the lock';
		return this.holderName || this.holderId;
	}

	constructor() {
		if (browser) {
			const owner = storedOwner();
			this.ownerId = owner.ownerId;
			this.ownerName = owner.ownerName;
		}
	}

	watch(workflowId: string): void {
		if (this.workflowId === workflowId && workflowId !== '') return;
		this.dispose();
		this.workflowId = workflowId;
		this.holderId = null;
		this.holderName = '';
		this.refreshError = null;
		if (!workflowId || !browser) return;
		void this.refresh();
		this.pollTimer = window.setInterval(() => {
			void this.refresh();
		}, POLL_MS);
	}

	async acquire(): Promise<boolean> {
		if (!this.workflowId) return false;
		if (!this.supported) return true;
		try {
			const lock = await acquireWorkflowLock(this.workflowId, {
				ownerId: this.ownerId,
				ownerName: this.ownerName,
			});
			this.applyHolder(lock.ownerId, lock.ownerName);
			this.refreshError = null;
			this.startHeartbeat();
			return lock.ownerId === this.ownerId;
		} catch (e) {
			if (this.isMissingEndpoint(e)) {
				this.supported = false;
				return true;
			}
			this.refreshError = e instanceof Error ? e.message : 'Lock failed.';
			await this.refresh();
			return false;
		}
	}

	async release(): Promise<void> {
		this.stopHeartbeat();
		if (!this.workflowId || !this.supported || !this.held) return;
		try {
			await releaseWorkflowLock(this.workflowId, this.ownerId);
		} catch {
			// Leaving edit never blocks on release failures.
		}
		this.holderId = null;
		this.holderName = '';
	}

	dispose(): void {
		this.stopHeartbeat();
		if (this.pollTimer !== null) {
			window.clearInterval(this.pollTimer);
			this.pollTimer = null;
		}
		this.workflowId = '';
	}

	private async refresh(): Promise<void> {
		if (!this.workflowId) return;
		try {
			const lock = await getWorkflowLock(this.workflowId);
			this.supported = true;
			this.applyHolder(lock?.ownerId ?? null, lock?.ownerName ?? '');
			this.refreshError = null;
		} catch (e) {
			if (this.isMissingEndpoint(e)) {
				this.supported = false;
				this.refreshError = null;
				return;
			}
			this.refreshError = e instanceof Error ? e.message : 'Lock query failed.';
		}
	}

	private startHeartbeat(): void {
		this.stopHeartbeat();
		if (!browser) return;
		this.heartbeatTimer = window.setInterval(() => {
			if (!this.workflowId || !this.held) return;
			void heartbeatWorkflowLock(this.workflowId, this.ownerId)
				.then((lock) => this.applyHolder(lock.ownerId, lock.ownerName))
				.catch(() => {
					void this.refresh();
				});
		}, HEARTBEAT_MS);
	}

	private stopHeartbeat(): void {
		if (this.heartbeatTimer !== null) {
			window.clearInterval(this.heartbeatTimer);
			this.heartbeatTimer = null;
		}
	}

	private applyHolder(ownerId: string | null, ownerName: string): void {
		this.holderId = ownerId;
		this.holderName = ownerName;
	}

	private isMissingEndpoint(e: unknown): boolean {
		const message = e instanceof Error ? e.message : String(e);
		return /404|not found|no route|unknown endpoint/i.test(message);
	}
}
