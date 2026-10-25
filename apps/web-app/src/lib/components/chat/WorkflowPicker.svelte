<script lang="ts">
	/**
	 * Workflow selector for the composer. It picks the workflow the
	 * workflow execution type runs: the graph itself is resolved from the
	 * template on the server, so nothing about the graph is assembled here.
	 */
	import InstancePicker, { type PickerOption } from '#lib/components/chat/InstancePicker.svelte';
	import { isRunnableTemplate, listTemplates } from '#lib/services/templates';
	import { pickerState } from '#lib/stores/picker-state.svelte';
	import { t } from '#lib/i18n';
	import type { Template } from '#lib/types/models';

	interface Props {
		value?: string;
		class?: string;
		activePicker?: string | null;
		pickerKey?: string;
		onActivate?: (key: string | null) => void;
		onClose?: () => void;
	}

	let {
		value = $bindable(''),
		class: className = '',
		activePicker = undefined,
		pickerKey = 'workflow',
		onActivate,
		onClose,
	}: Props = $props();

	let templates = $state<Template[]>([]);

	$effect(() => {
		void pickerState.load();
		void listTemplates({ kind: 'workflow' })
			.then((items) => {
				templates = items.filter(isRunnableTemplate);
			})
			.catch(() => {
				templates = [];
			});
	});

	const options = $derived<PickerOption[]>(
		templates.map((template) => ({
			id: template.id,
			label: template.name || template.id,
			detail: `${template.id}${template.category ? ` · ${template.category}` : ''}`,
		})),
	);
</script>

<InstancePicker
	bind:value
	{options}
	pinnedIds={pickerState.pinnedIds('workflow')}
	recentIds={pickerState.recentIds('workflow')}
	ariaLabel={t('chat.workflowPicker.ariaLabel')}
	placeholder={t('chat.workflowPicker.placeholder')}
	filterPlaceholder={t('chat.workflowPicker.filterPlaceholder')}
	emptyLabel={t('chat.workflowPicker.emptyLabel')}
	icon="workflow"
	triggerClass="w-40"
	{activePicker}
	{pickerKey}
	{onActivate}
	{onClose}
	onPin={(id, pinned) => void pickerState.setPinned('workflow', id, pinned)}
	class={className}
/>
