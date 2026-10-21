<script lang="ts">
	/**
	 * Model selector for the composer. Lists the registered model profiles
	 * for one-click selection and keeps free-form entry as a fallback for
	 * profiles the list does not cover: the filter box doubles as a manual
	 * value, and Enter commits whatever is typed.
	 */
	import InstancePicker, {
		type PickerOption,
	} from '#lib/components/chat/InstancePicker.svelte';
	import { listModelProfiles } from '#lib/services/resources';
	import { pickerState } from '#lib/stores/picker-state.svelte';
	import { t } from '#lib/i18n';
	import type { ModelProfile } from '#lib/types/models';

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
		pickerKey = 'model',
		onActivate,
		onClose,
	}: Props = $props();

	let profiles = $state<ModelProfile[]>([]);

	$effect(() => {
		void pickerState.load();
		void listModelProfiles()
			.then((items) => {
				profiles = items;
			})
			.catch(() => {
				profiles = [];
			});
	});

	const options = $derived<PickerOption[]>(
		profiles.map((profile) => ({
			id: profile.id,
			label: profile.name || profile.model,
			detail: `${profile.id}${profile.provider ? ` · ${profile.provider}` : ''}`,
			badge: profile.isDefault ? t('chat.instancePicker.defaultBadge') : undefined,
		})),
	);
</script>

<InstancePicker
	bind:value
	options={options}
	pinnedIds={pickerState.pinnedIds('model')}
	recentIds={pickerState.recentIds('model')}
	allowFreeform
	ariaLabel={t('chat.modelPicker.ariaLabel')}
	placeholder={t('chat.modelPicker.placeholder')}
	filterPlaceholder={t('chat.modelPicker.filterPlaceholder')}
	emptyLabel={t('chat.modelPicker.emptyLabel')}
	icon="cpu"
	triggerClass="w-44"
	{activePicker}
	{pickerKey}
	{onActivate}
	{onClose}
	onPin={(id, pinned) => void pickerState.setPinned('model', id, pinned)}
	class={className}
/>
