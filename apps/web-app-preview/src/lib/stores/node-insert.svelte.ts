/**
 * A node queued for the workflow canvas. `nodeType` is already normalised to
 * what the backend stores — a builtin name or a plugin-contributed one — so
 * the consumer never has to interpret a template.
 */
export interface PendingNodeInsert {
	nodeType: string;
	name: string;
}

/**
 * Pending node insertion aimed at the workflow editor. The template detail
 * page sets it through the "Insert into canvas" action; the workflow edit tab
 * consumes it once on mount so the node lands on the active canvas.
 *
 * The queue lives in memory only: a reload before reaching the editor drops
 * it, which is why the action navigates straight to the workflow list.
 */
class NodeInsertStore {
	private pending = $state<PendingNodeInsert | null>(null);

	get value(): PendingNodeInsert | null {
		return this.pending;
	}

	set(node: PendingNodeInsert): void {
		this.pending = node;
	}

	consume(): PendingNodeInsert | null {
		const current = this.pending;
		this.pending = null;
		return current;
	}

	clear(): void {
		this.pending = null;
	}
}

export const nodeInsertStore = new NodeInsertStore();
