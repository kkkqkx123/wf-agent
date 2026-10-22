import { browser } from '$app/env';
import {
	acquireDefinitionLock,
	getDefinitionLock,
	heartbeatDefinitionLock,
	releaseDefinitionLock,
} from '#lib/services/definitions';
import type { DefinitionKind } from '#lib/types/models';

const OWNER_KEY = 'wf-definition-lock-owner';
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
 * Single-write lock state for one definition. The edit store stays unaware
 * of locking; pages gate entering edit, saving and promoting on this
 * store. The lease is keyed by the definition kind as well as its id, so a
 * workflow and an agent never contend for the same lease.
 *
 * Transport: REST poll plus holder heartbeat. The WS protocol carries no
 * lock events yet, so there is no push subscription here; `notifyRemoteHint`
 * is the seam a future lock event will call, and it simply re-reads the
 * holder through the same REST path.
 *
 * When the lock endpoints are absent the store reports unsupported and
 * leaves editing unblocked with a notice.
 */
export class EditLockStore {
	holderId = $state<string | null>(null);
	holderName = $state('');
	supported = $state(true);
	refreshError = $state<string | null>(null);
	/** A previously held lease changed hands or expired. Sticky until re-acquired or acknowledged. */
	lockLost = $state(false);
	/** Who holds the lease at loss time; empty when the lease simply expired. */
	lockLostBy = $state('');

	private kind: DefinitionKind;
	private definitionId = $state('');
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

	constructor(kind: DefinitionKind) {
		this.kind = kind;
		if (browser) {
			const owner = storedOwner();
			this.ownerId = owner.ownerId;
			this.ownerName = owner.ownerName;
		}
	}

	watch(definitionId: string): void {
		if (this.definitionId === definitionId && definitionId !== '') return;
		// Switching definitions releases the previous lease without blocking;
		// failures only surface as a notice.
		if (this.definitionId && this.supported && this.held) {
			const previousId = this.definitionId;
			const previousOwner = this.ownerId;
			void releaseDefinitionLock(this.kind, previousId, previousOwner).catch((e) => {
				this.refreshError =
					e instanceof Error ? e.message : 'Lock release failed.';
			});
		}
		this.dispose();
		this.definitionId = definitionId;
		this.holderId = null;
		this.holderName = '';
		this.refreshError = null;
		this.leaseActive = false;
		this.acknowledgeLockLoss();
		if (!definitionId || !browser) return;
		void this.refresh();
		this.pollTimer = window.setInterval(() => {
			void this.refresh();
		}, POLL_MS);
	}

	async acquire(): Promise<boolean> {
		if (!this.definitionId) return false;
		if (!this.supported) return true;
		try {
			const lock = await acquireDefinitionLock(this.kind, this.definitionId, {
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
		if (!this.definitionId || !this.supported || !this.held) return;
		try {
			await releaseDefinitionLock(this.kind, this.definitionId, this.ownerId);
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
		this.definitionId = '';
	}

	/** Clear a recorded lease loss after the user acted on it. */
	acknowledgeLockLoss(): void {
		this.lockLost = false;
		this.lockLostBy = '';
	}

	/**
	 * Realtime seam for a future lock event. Until the backend dispatches
	 * lock changes, push callers invoke this to re-read the holder; unknown
	 * event types are ignored so the REST poll stays authoritative.
	 */
	notifyRemoteHint(eventType?: string): void {
		if (!eventType) {
			void this.refresh();
			return;
		}
		const normalized = eventType.trim().toUpperCase();
		if (
			normalized.includes('LOCK') ||
			normalized === 'NOTIFICATION_SENT' ||
			normalized === 'ERROR'
		) {
			void this.refresh();
		}
	}

	/** Force one holder re-read, used after local acquire/release and hints. */
	refreshNow(): void {
		void this.refresh();
	}

	private async refresh(): Promise<void> {
		if (!this.definitionId) return;
		try {
			const lock = await getDefinitionLock(this.kind, this.definitionId);
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
			if (!this.definitionId || !this.held) return;
			void heartbeatDefinitionLock(this.kind, this.definitionId, this.ownerId)
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
