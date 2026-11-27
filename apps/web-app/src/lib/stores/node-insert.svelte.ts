import { normalizeNodeType } from '#lib/graph/node-kind';

/**
 * A node queued for the workflow canvas. `nodeType` is already normalised to
 * what the backend stores — a builtin name or a plugin-contributed one — so
 * the consumer never has to interpret a template.
 */
export interface PendingNodeInsert {
	nodeType: string;
	name: string;
	/**
	 * Target workflow for a deep link. Absent means the user still picks a
	 * workflow from the list; present navigates straight to that workflow's
	 * edit tab.
	 */
	targetWorkflowId?: string | null;
	/** Template the insert was queued from, used as the node template reference. */
	sourceTemplateId?: string | null;
	/** When the insert was queued, for expiry and diagnostics. */
	queuedAt: number;
}

const STORAGE_KEY = 'wf.pending-node-insert.v1';

/** Inserts older than this are stale intents and must not be replayed. */
export const NODE_INSERT_MAX_AGE_MS = 30 * 60 * 1000;

function text(value: unknown): string | null {
	return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function normalizeEntry(value: unknown, now: number = Date.now()): PendingNodeInsert | null {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
	const record = value as Record<string, unknown>;
	const nodeType = normalizeNodeType(text(record.nodeType) ?? '');
	if (nodeType === null) return null;
	const queuedAt =
		typeof record.queuedAt === 'number' && Number.isFinite(record.queuedAt) ? record.queuedAt : now;
	if (queuedAt <= 0 || queuedAt - now > 60_000 || now - queuedAt > NODE_INSERT_MAX_AGE_MS) {
		return null;
	}
	return {
		nodeType,
		name: text(record.name) ?? '',
		targetWorkflowId: text(record.targetWorkflowId),
		sourceTemplateId: text(record.sourceTemplateId),
		queuedAt,
	};
}

/**
 * Pending node insertion aimed at the workflow editor. The template detail
 * page sets it through the "Insert into canvas" action; the workflow edit tab
 * consumes it once on mount so the node lands on the active canvas.
 *
 * The queue lives in memory with a `sessionStorage` backup: a reload before
 * reaching the editor restores from storage instead of dropping silently.
 * An expired or malformed entry is removed on read, and the editor clears it
 * only after the insertion has been applied to the session document.
 */
class NodeInsertStore {
	private pending = $state<PendingNodeInsert | null>(null);

	set(node: Omit<PendingNodeInsert, 'queuedAt'> & { queuedAt?: number }): void {
		const entry = normalizeEntry({ ...node, queuedAt: node.queuedAt ?? Date.now() });
		if (entry === null) throw new Error('Pending node insert requires a non-blank node type');
		this.pending = entry;
		try {
			if (typeof sessionStorage !== 'undefined') {
				sessionStorage.setItem(STORAGE_KEY, JSON.stringify(entry));
			}
		} catch {
			// Storage is best-effort; memory still carries the insert.
		}
	}

	peek(): PendingNodeInsert | null {
		if (this.pending) {
			const current = normalizeEntry(this.pending);
			if (current) return current;
			this.pending = null;
			this.removeStorage();
			return null;
		}
		try {
			if (typeof sessionStorage === 'undefined') return null;
			const raw = sessionStorage.getItem(STORAGE_KEY);
			if (!raw) return null;
			const restored = normalizeEntry(JSON.parse(raw) as unknown);
			if (restored === null) {
				this.removeStorage();
				return null;
			}
			this.pending = restored;
			return restored;
		} catch {
			return null;
		}
	}

	clear(): void {
		this.pending = null;
		this.removeStorage();
	}

	private removeStorage(): void {
		try {
			if (typeof sessionStorage !== 'undefined') {
				sessionStorage.removeItem(STORAGE_KEY);
			}
		} catch {
			// Clearing storage is best-effort.
		}
	}
}

export const nodeInsertStore = new NodeInsertStore();
