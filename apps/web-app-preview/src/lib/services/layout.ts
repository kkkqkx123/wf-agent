import { client } from '$lib/api/client';
import { call } from '$lib/api/envelope';

/** A single persisted node position (grid-snapped canvas coordinates). */
export interface LayoutPoint {
	x: number;
	y: number;
}

/** Layout algorithm, mirroring the backend `LayoutKind` enum. */
export type LayoutKind = 'layered' | 'columns' | 'rows' | 'compact';

/** Layout direction, mirroring the backend `LayoutDirection` enum. */
export type LayoutDirection = 'lr' | 'tb' | 'rl' | 'bt';

/** Layout settings persisted alongside positions. */
export interface LayoutSettings {
	kind: LayoutKind;
	direction?: LayoutDirection;
	rank_sep?: number;
	node_sep?: number;
	row_height?: number;
	column_width?: number;
	wrap_count?: number;
	padding?: number;
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
		client.DELETE('/api/v1/workflows/{id}/layout', { params: { path: { id } } }),
	);
}
