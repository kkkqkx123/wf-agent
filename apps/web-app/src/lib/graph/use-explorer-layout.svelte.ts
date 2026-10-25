import type { CanvasPosition } from '#lib/graph/canvas-model';
import type { GraphLayoutKind } from '#lib/graph/display-model';
import { t } from '#lib/i18n';
import { getWorkflowLayout, resetWorkflowLayout, saveWorkflowLayout } from '#lib/services/layout';
import { toasts } from '#lib/stores/toast.svelte';

/**
 * Persisted canvas layout backing the explorer. Positions saved on the
 * backend load once per workflow id; view-mode drags update them with a
 * debounced save. Construction and persistence stay here so the explorer
 * component only owns selection, filtering and canvas wiring.
 */
export class ExplorerLayoutState {
	savedPositions = $state<Record<string, CanvasPosition>>({});

	private loadedId: string | null = null;
	private saveTimer: ReturnType<typeof setTimeout> | null = null;

	/** Load persisted positions once per workflow id and apply its kind. */
	ensureLoaded(layoutId: string | undefined, applyKind: (kind: GraphLayoutKind) => void): void {
		if (!layoutId || this.loadedId === layoutId) return;
		this.loadedId = layoutId;
		void getWorkflowLayout(layoutId)
			.then((saved) => {
				if (!saved) return;
				const positions: Record<string, CanvasPosition> = {};
				for (const [nodeId, point] of Object.entries(saved.positions ?? {})) {
					positions[nodeId] = { x: point.x, y: point.y };
				}
				this.savedPositions = positions;
				const kind = saved.settings?.kind;
				if (kind === 'layered' || kind === 'columns' || kind === 'rows' || kind === 'compact') {
					applyKind(kind);
				}
			})
			.catch(() => {
				// Layout persistence is optional; the preset layout still renders.
			});
	}

	/** Record a view-mode drag and debounce the backend save. */
	handleViewMove(
		layoutId: string | undefined,
		editMode: boolean,
		kind: GraphLayoutKind,
		id: string,
		position: CanvasPosition,
	): void {
		if (!layoutId || editMode) return;
		this.savedPositions = { ...this.savedPositions, [id]: position };
		if (this.saveTimer) clearTimeout(this.saveTimer);
		this.saveTimer = setTimeout(() => {
			this.saveTimer = null;
			void saveWorkflowLayout(layoutId, {
				positions: this.savedPositions,
				settings: { kind },
			}).catch(() => {
				toasts.error(t('domain.graph.layoutSaveFailed'), t('domain.graph.layoutSaveFailedDetail'));
			});
		}, 800);
	}

	/** Drop persisted positions and ask the canvas to recompute them. */
	async resetLayout(layoutId: string | undefined, relayout: () => void): Promise<void> {
		if (!layoutId) return;
		if (this.saveTimer) {
			clearTimeout(this.saveTimer);
			this.saveTimer = null;
		}
		this.savedPositions = {};
		relayout();
		try {
			await resetWorkflowLayout(layoutId);
		} catch {
			toasts.error(t('domain.graph.layoutResetFailed'), t('domain.graph.layoutResetFailedDetail'));
		}
	}

	dispose(): void {
		if (this.saveTimer) {
			clearTimeout(this.saveTimer);
			this.saveTimer = null;
		}
	}
}
