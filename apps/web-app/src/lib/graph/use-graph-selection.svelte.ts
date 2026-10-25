/**
 * Runes selection state for graph explorers. The canvas stays the renderer;
 * this object owns the ordered navigation over the filtered node list so
 * keyboard moves and programmatic focus share one implementation.
 */
export class GraphSelectionState {
	selectedId = $state<string | null>(null);

	select(id: string | null): void {
		this.selectedId = id;
	}

	clear(): void {
		this.selectedId = null;
	}

	/**
	 * Move the selection across an ordered id list with wraparound.
	 * Returns the newly selected id, or null when the list is empty.
	 */
	move(ids: string[], delta: number): string | null {
		if (ids.length === 0) {
			this.selectedId = null;
			return null;
		}
		const current = this.selectedId ? ids.indexOf(this.selectedId) : -1;
		const next = ids[(current + delta + ids.length) % ids.length] ?? null;
		this.selectedId = next;
		return next;
	}

	/** Whether an id is the current selection. */
	isSelected(id: string): boolean {
		return this.selectedId === id;
	}
}

/** Create an isolated selection state for one graph view. */
export function useGraphSelection(): GraphSelectionState {
	return new GraphSelectionState();
}
