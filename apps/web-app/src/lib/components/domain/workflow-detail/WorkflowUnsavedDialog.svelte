<script lang="ts">
	import UnsavedChangesDialog from '#lib/ui/components/UnsavedChangesDialog.svelte';
	import type { WorkflowEditSession } from '#lib/workflows/edit-session.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		controller: WorkflowEditSession;
	}

	let { controller }: Props = $props();
</script>

<UnsavedChangesDialog
	bind:open={controller.unsavedOpen}
	description={t('domain.workflowUnsaved.description')}
	saveLabel={t('domain.workflowUnsaved.saveAndLeave')}
	busy={controller.unsavedBusy}
	ondiscard={() => controller.discardWorkflowEdits()}
	onsave={() => void controller.saveAndProceed()}
/>
