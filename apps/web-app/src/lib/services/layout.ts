import { client } from '#lib/api/client';
import { call } from '#lib/api/envelope';
import type { GraphLayoutKind } from '#lib/graph/display-model';

/**
 * Layout semantics: workflow persisted, template temporary.
 *
 * - Workflow definition graphs persist positions through the layout endpoint
 *   (`GET/PUT/DELETE /workflows/{id}/layout`). The edit tab saves topology
 *   via drafts and positions via this endpoint independently (`dirty` vs
 *   `layoutDirty`); the read-only graph tab autosaves view drags debounced.
 * - Template workflow graphs keep positions as ephemeral canvas state only.
 *   They have no layout endpoint by design, `layoutDirty` stays unwired, and
 *   pure moves are dropped when the session folds back to text.
 *
 * `LayoutKind` mirrors `GraphLayoutKind` from the canvas display model; the
 * canvas owns the canonical union and this module reuses it so the two never
 * drift. Only `kind` is persisted today.
 */
export type LayoutKind = GraphLayoutKind;

/** A single persisted node position (grid-snapped canvas coordinates). */
export interface LayoutPoint {
	x: number;
	y: number;
}

/**
 * Layout settings persisted alongside positions.
 */
export interface LayoutSettings {
	kind: LayoutKind;
}

/** Persisted canvas layout for a workflow definition. */
export interface WorkflowLayout {
	positions: Record<string, LayoutPoint>;
	settings?: LayoutSettings;
	updated_at?: number;
}

/** Load the persisted canvas layout of a workflow (null when never saved). */
export async function getWorkflowLayout(id: string): Promise<WorkflowLayout | null> {
	return call<WorkflowLayout | null>(
		client.GET('/api/v1/workflows/{id}/layout', { params: { path: { id } } }),
	);
}

/** Persist the canvas layout of a workflow (display data, not topology). */
export async function saveWorkflowLayout(
	id: string,
	layout: WorkflowLayout,
): Promise<WorkflowLayout | null> {
	return call<WorkflowLayout | null>(
		client.PUT('/api/v1/workflows/{id}/layout', {
			params: { path: { id } },
			body: layout,
		}),
	);
}

/** Remove the persisted canvas layout; the graph falls back to its default. */
export async function resetWorkflowLayout(id: string): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/workflows/{id}/layout', {
			params: { path: { id } },
		}),
	);
}
