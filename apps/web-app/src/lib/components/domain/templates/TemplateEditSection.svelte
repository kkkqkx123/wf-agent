<script lang="ts">
	import TemplateEditPanel from '$lib/components/domain/TemplateEditPanel.svelte';
	import type { TemplateDetail } from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';
	import type { TemplateEditController } from '$lib/templates/template-edit-session.svelte';

	interface Props {
		kind: TemplateKind;
		/** Loaded template detail, kept for context and future wiring. */
		detail: TemplateDetail;
		controller: TemplateEditController;
	}

	let { kind, controller }: Props = $props();
</script>

<TemplateEditPanel
	{kind}
	tab={controller.editTab}
	isNew={false}
	validationState={controller.validationState}
	fields={controller.formFields}
	bind:formState={controller.formState}
	bind:editText={controller.editText}
	syntaxError={controller.syntaxError}
	serverIssues={controller.serverIssues}
	session={controller.session}
	store={controller.templateEditStore}
	draftBusy={controller.draftBusy}
	saveBusy={controller.saveBusy}
	openWorkflowBusy={controller.openWorkflowBusy}
	graphRequest={controller.graphRequest}
	ontabchange={(next) => (controller.editTab = next)}
	onvalidate={() => void controller.runValidate()}
	onsave={() => void controller.runSave()}
	oncancel={() => controller.exitEdit()}
	onservercheck={() => void controller.runTemplateServerGate()}
	onopenworkflow={() => void controller.runOpenAsWorkflow()}
	onmovenode={(id, position) => controller.handleTemplateMoveNode(id, position)}
	onmovenodes={(moves) => controller.handleTemplateMoveNodes(moves)}
	onaddnode={(position) => controller.handleTemplateAddNode(position)}
	onconnect={(source, target) =>
		controller.handleTemplateConnect(source, target)}
	ondeletenodes={(ids) => controller.handleTemplateDeleteNodes(ids)}
	ondeletegroups={(ids) => controller.handleTemplateDeleteGroups(ids)}
/>
