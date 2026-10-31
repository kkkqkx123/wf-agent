<script lang="ts">
	/**
	 * Execution type selector for the composer. The entry answers one
	 * question: which executor runs the message. Agent runs the full loop
	 * with tools, Chat answers tool-free multi-turn questions, and Workflow
	 * hands the message to a graph run configured on the workflow surface.
	 * Rendered through the shared instance picker so trigger, panel and
	 * keyboard behavior stay identical to the agent/model/workflow pickers.
	 */
	import InstancePicker, { type PickerOption } from '#lib/components/chat/InstancePicker.svelte';
	import { t } from '#lib/i18n';

	/** The executors the composer can hand a message to. */
	export type ExecutionType = 'agent' | 'workflow' | 'chat';

	interface Props {
		value?: ExecutionType;
		class?: string;
		activePicker?: string | null;
		pickerKey?: string;
		onActivate?: (key: string | null) => void;
		onClose?: () => void;
	}

	let {
		value = $bindable('agent'),
		class: className = '',
		activePicker = undefined,
		pickerKey = 'execution',
		onActivate,
		onClose,
	}: Props = $props();

	const options = $derived<PickerOption[]>([
		{
			id: 'agent',
			label: t('chat.executionTypePicker.labelAgent'),
			detail: t('chat.executionTypePicker.detailAgent'),
			icon: 'cpu',
		},
		{
			id: 'workflow',
			label: t('chat.executionTypePicker.labelWorkflow'),
			detail: t('chat.executionTypePicker.detailWorkflow'),
			icon: 'workflow',
		},
		{
			id: 'chat',
			label: t('chat.executionTypePicker.labelChat'),
			detail: t('chat.executionTypePicker.detailChat'),
			icon: 'sparkles',
		},
	]);
</script>

<InstancePicker
	bind:value
	{options}
	ariaLabel={t('chat.executionTypePicker.ariaLabel')}
	placeholder={t('chat.executionTypePicker.placeholder')}
	filterPlaceholder={t('chat.executionTypePicker.filterPlaceholder')}
	emptyLabel={t('chat.executionTypePicker.emptyLabel')}
	icon="cpu"
	triggerClass="w-32"
	{activePicker}
	{pickerKey}
	{onActivate}
	{onClose}
	class={className}
/>
