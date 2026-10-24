<script lang="ts">
	import WorkflowEditPanel from '#lib/components/domain/WorkflowEditPanel.svelte';
	import type { WorkflowEditSession } from '#lib/workflows/edit-session.svelte';
	import { presence } from '#lib/stores/presence.svelte';

	interface Props {
		controller: WorkflowEditSession;
		/** Owning workflow id, kept for context and future wiring. */
		workflowId: string;
	}

	let { controller }: Props = $props();
</script>

<p class="mb-2 text-micro text-muted-foreground">
	Shared definition editor · this canvas, lock, draft, validate and promote flow
	is the same one workflow templates use; the workflow page is the runnable
	instance view, not a second definition source.
</p>
<WorkflowEditPanel
	store={controller.editStore}
	lock={controller.lockStore}
	editMode={controller.editMode}
	editBusy={controller.editBusy}
	layoutBusy={controller.layoutBusy}
	issues={controller.editTemplateIssues}
	onenteredit={() => void controller.enterEdit()}
	onexitedit={() => controller.exitEdit()}
	onrelock={() => void controller.enterEdit()}
	onsave={() => void controller.saveEditDraft()}
	onsavelayout={() => void controller.saveLayout()}
	onresetlayout={() => void controller.resetLayout()}
	onvalidate={() => void controller.validateEditDraft()}
	onpromote={() => void controller.promoteEditDraft()}
	onmovenode={(id, position) => controller.handleMoveNode(id, position)}
	onmovenodes={(moves) => controller.handleMoveNodes(moves)}
	onaddnode={(position, nodeType, name) =>
		controller.handleAddNode(position, nodeType, name)}
	ondeleteedge={(id) => controller.handleDeleteEdge(id)}
	onconnect={(source, target) => controller.handleConnect(source, target)}
	ondeletenodes={(ids) => controller.handleDeleteNodes(ids)}
	ondeletegroups={(ids) => controller.handleDeleteGroups(ids)}
	presence={presence.peers}
	oncursormove={(position) => presence.move(position.x, position.y)}
/>
