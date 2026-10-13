<script lang="ts">
	import UnsavedChangesDialog from '@wf-agent/ui/components/UnsavedChangesDialog.svelte';
	import type { WorkflowEditSession } from '$lib/workflows/edit-session.svelte';

	interface Props {
		controller: WorkflowEditSession;
	}

	let { controller }: Props = $props();
</script>

<UnsavedChangesDialog
	bind:open={controller.unsavedOpen}
	description="The edit canvas has unsaved draft changes. Leaving discards the canvas edits."
	saveLabel="Save draft & leave"
	busy={controller.unsavedBusy}
	ondiscard={() => controller.discardWorkflowEdits()}
	onsave={() => void controller.saveAndProceed()}
/>
