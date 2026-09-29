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
	/** A previously held lease changed hands or expired. Sticky until re-acquired or acknowledged. */
	lockLost = $state(false);
	/** Who holds the lease at loss time; empty when the lease simply expired. */
	lockLostBy = $state('');

	private workflowId = $state('');
	private ownerId = $state('');
	private ownerName = $state('');
	private leaseActive = false;
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
			this.supported && this.holderId !== null && this.holderId !== this.ownerId
		);
	}

	/** Poll or heartbeat failed: holder is stale, writes must wait. */
	get unknown(): boolean {
		return this.supported && this.refreshError !== null;
	}

	get canWrite(): boolean {
		if (!this.supported) return true;
		if (this.refreshError !== null) return false;
		return this.held;
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
		// Switching workflows releases the previous lease without blocking;
		// failures only surface as a notice.
		if (this.workflowId && this.supported && this.held) {
			const previousId = this.workflowId;
			const previousOwner = this.ownerId;
			void releaseWorkflowLock(previousId, previousOwner).catch((e) => {
				this.refreshError =
					e instanceof Error ? e.message : 'Lock release failed.';
			});
		}
		this.dispose();
		this.workflowId = workflowId;
		this.holderId = null;
		this.holderName = '';
		this.refreshError = null;
		this.leaseActive = false;
		this.acknowledgeLockLoss();
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
			this.leaseActive = lock.ownerId === this.ownerId;
			if (this.leaseActive) this.acknowledgeLockLoss();
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
		this.leaseActive = false;
		if (!this.workflowId || !this.supported || !this.held) return;
		try {
			await releaseWorkflowLock(this.workflowId, this.ownerId);
			this.refreshError = null;
		} catch (e) {
			// Leaving edit never blocks on release failures.
			this.refreshError =
				e instanceof Error ? e.message : 'Lock release failed.';
		}
		this.holderId = null;
		this.holderName = '';
	}

	dispose(): void {
		this.stopHeartbeat();
		this.leaseActive = false;
		if (this.pollTimer !== null) {
			window.clearInterval(this.pollTimer);
			this.pollTimer = null;
		}
		this.workflowId = '';
	}

	/** Clear a recorded lease loss after the user acted on it. */
	acknowledgeLockLoss(): void {
		this.lockLost = false;
		this.lockLostBy = '';
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
		// Our own release clears the holder first, so a held-to-foreign
		// transition here always means the lease was stolen or expired.
		if (this.leaseActive && this.ownerId !== '' && ownerId !== this.ownerId) {
			this.lockLost = true;
			this.lockLostBy = ownerName;
			this.leaseActive = false;
		}
		this.holderId = ownerId;
		this.holderName = ownerName;
	}

	private isMissingEndpoint(e: unknown): boolean {
		const message = e instanceof Error ? e.message : String(e);
		return /404|not found|no route|unknown endpoint/i.test(message);
	}
}
