<script lang="ts">
	import TemplateEditPanel from '#lib/components/domain/TemplateEditPanel.svelte';
	import type { TemplateKind } from '#lib/types/models';
	import type { TemplateEditController } from '#lib/templates/template-edit-session.svelte';

	interface Props {
		kind: TemplateKind;
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
	graphRequest={controller.graphRequest}
	lockHolder={controller.lockHolder}
	canWrite={controller.canWrite}
	ontabchange={(next) => (controller.editTab = next)}
	onvalidate={() => void controller.runValidate()}
	onsave={() => void controller.runSave()}
	oncancel={() => controller.exitEdit()}
	onservercheck={() => void controller.runTemplateServerGate()}
	onmovenode={(id, position) => controller.handleTemplateMoveNode(id, position)}
	onmovenodes={(moves) => controller.handleTemplateMoveNodes(moves)}
	onaddnode={(position) => controller.handleTemplateAddNode(position)}
	onconnect={(source, target) => controller.handleTemplateConnect(source, target)}
	ondeletenodes={(ids) => controller.handleTemplateDeleteNodes(ids)}
	ondeletegroups={(ids) => controller.handleTemplateDeleteGroups(ids)}
/>
