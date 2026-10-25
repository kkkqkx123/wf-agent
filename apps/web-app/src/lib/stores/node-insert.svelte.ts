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
	/** Template the insert was queued from, for recoverable prompts. */
	sourceTemplateId?: string | null;
	/** When the insert was queued, for expiry and diagnostics. */
	queuedAt: number;
}

const STORAGE_KEY = 'wf.pending-node-insert.v1';

/**
 * Pending node insertion aimed at the workflow editor. The template detail
 * page sets it through the "Insert into canvas" action; the workflow edit tab
 * consumes it once on mount so the node lands on the active canvas.
 *
 * The queue lives in memory with a `sessionStorage` backup: a reload before
 * reaching the editor restores from storage instead of dropping silently.
 * When both are empty the workflow page shows a recoverable prompt that
 * links back to the template instead of failing silently.
 */
class NodeInsertStore {
	private pending = $state<PendingNodeInsert | null>(null);

	get value(): PendingNodeInsert | null {
		return this.pending;
	}

	set(node: Omit<PendingNodeInsert, 'queuedAt'> & { queuedAt?: number }): void {
		const entry: PendingNodeInsert = {
			...node,
			queuedAt: node.queuedAt ?? Date.now(),
		};
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
		if (this.pending) return this.pending;
		try {
			if (typeof sessionStorage === 'undefined') return null;
			const raw = sessionStorage.getItem(STORAGE_KEY);
			if (!raw) return null;
			const parsed = JSON.parse(raw) as PendingNodeInsert;
			if (!parsed || typeof parsed.nodeType !== 'string' || !parsed.nodeType) {
				return null;
			}
			this.pending = {
				nodeType: parsed.nodeType,
				name: typeof parsed.name === 'string' ? parsed.name : '',
				targetWorkflowId:
					typeof parsed.targetWorkflowId === 'string'
						? parsed.targetWorkflowId
						: null,
				sourceTemplateId:
					typeof parsed.sourceTemplateId === 'string'
						? parsed.sourceTemplateId
						: null,
				queuedAt:
					typeof parsed.queuedAt === 'number' ? parsed.queuedAt : Date.now(),
			};
			return this.pending;
		} catch {
			return null;
		}
	}

	consume(): PendingNodeInsert | null {
		const current = this.peek();
		this.pending = null;
		try {
			if (typeof sessionStorage !== 'undefined') {
				sessionStorage.removeItem(STORAGE_KEY);
			}
		} catch {
			// Clearing storage is best-effort.
		}
		return current;
	}

	clear(): void {
		this.pending = null;
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
