import { listWorkflows } from '$lib/services/workflows';
import { createResource } from '$lib/stores/collection.svelte';
import { shortId } from '$lib/utils/format';

/** The shared page ceiling, so one request covers the workflow definitions. */
const INDEX_LIMIT = 500;

/**
 * An execution only names its workflow by id, so the views that show a run's
 * workflow read the title from this single index.
 */
const titles = createResource(async () => {
	const page = await listWorkflows({ limit: INDEX_LIMIT });
	const index: Record<string, string> = {};
	for (const workflow of page.items) index[workflow.id] = workflow.name;
	return index;
});

export function loadWorkflowTitles(): Promise<void> {
	return titles.reload();
}

/** Title of a workflow, or the short form of its id while it is unknown. */
export function workflowTitle(workflowId: string): string {
	return titles.data?.[workflowId] ?? shortId(workflowId, 8);
}
