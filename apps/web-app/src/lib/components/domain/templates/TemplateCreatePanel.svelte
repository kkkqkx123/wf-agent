<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import TemplateEditPanel from '$lib/components/domain/TemplateEditPanel.svelte';
	import { templateFormFields } from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';
	import type { TemplateEditController } from '$lib/templates/template-edit-session.svelte';

	interface Props {
		controller: TemplateEditController;
	}

	let { controller }: Props = $props();
</script>

<div class="grid grid-cols-2 gap-2">
	<Select
		value={controller.createKind}
		options={controller.kindOptions}
		size="sm"
		placeholder="Kind"
		onchange={(value) => {
			const next = value as TemplateKind;
			if (next === controller.createKind) return;
			/* eslint-disable svelte/no-navigation-without-resolve -- resolve() cannot append the query string */
			void goto(
				resolve('/templates/[kind]/[id]', { kind: next, id: 'new' }) +
					'?tab=edit',
			);
			/* eslint-enable svelte/no-navigation-without-resolve */
		}}
	/>
	<input
		bind:value={controller.createId}
		placeholder="Template id"
		class="h-7 rounded-md border border-input bg-card px-2.5 text-small"
	/>
</div>
<input
	bind:value={controller.createName}
	placeholder="Template name"
	class="h-8 w-full rounded-md border border-input bg-card px-2.5 text-body"
/>
<TemplateEditPanel
	kind={controller.createKind}
	tab={controller.editTab}
	isNew
	validationState={controller.validationState}
	fields={templateFormFields(controller.createKind)}
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
	oncancel={() => void controller.backToLibrary()}
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
