<script lang="ts">
	/**
	 * Agent selector for the composer. Lists runnable agent templates for
	 * one-click selection and keeps free-form entry as a fallback.
	 */
	import InstancePicker, {
		type PickerOption,
	} from '$lib/components/chat/InstancePicker.svelte';
	import { isRunnableTemplate, listTemplates, DEFAULT_AGENT_ID } from '$lib/services/templates';
	import { pickerState } from '$lib/stores/picker-state.svelte';
	import { t } from '$lib/i18n';
	import type { Template } from '$lib/types/models';

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
		pickerKey = 'agent',
		onActivate,
		onClose,
	}: Props = $props();

	let templates = $state<Template[]>([]);

	$effect(() => {
		void pickerState.load();
		void listTemplates({ kind: 'agent' })
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
			badge: template.id === DEFAULT_AGENT_ID ? t('chat.instancePicker.defaultBadge') : undefined,
		})),
	);
</script>

<InstancePicker
	bind:value
	options={options}
	pinnedIds={pickerState.pinnedIds('agent')}
	recentIds={pickerState.recentIds('agent')}
	allowFreeform
	ariaLabel={t('chat.agentPicker.ariaLabel')}
	placeholder={t('chat.agentPicker.placeholder')}
	filterPlaceholder={t('chat.agentPicker.filterPlaceholder')}
	emptyLabel={t('chat.agentPicker.emptyLabel')}
	icon="cpu"
	triggerClass="w-40"
	{activePicker}
	{pickerKey}
	{onActivate}
	{onClose}
	onPin={(id, pinned) => void pickerState.setPinned('agent', id, pinned)}
	class={className}
/>
