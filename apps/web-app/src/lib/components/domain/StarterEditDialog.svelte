<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Switch from '#lib/ui/components/Switch.svelte';
	import Textarea from '#lib/ui/components/Textarea.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import { t } from '#lib/i18n';
	import type { ChatStarter, StarterInput } from '#lib/services/starters';

	interface Props {
		open: boolean;
		/** Null opens the dialog in create mode. */
		starter: ChatStarter | null;
		busy: boolean;
		error: string | null;
		onsave: (input: StarterInput) => void;
	}

	let { open = $bindable(false), starter, busy, error, onsave }: Props = $props();

	const isEdit = $derived(starter !== null);

	let title = $state('');
	let content = $state('');
	let locale = $state('en');
	let tags = $state('');
	let enabled = $state(true);
	let isDefault = $state(false);

	// Prefill whenever a different starter is opened for editing.
	$effect(() => {
		if (!open) return;
		title = starter?.title ?? '';
		content = starter?.content ?? '';
		locale = starter?.locale ?? 'en';
		tags = (starter?.tags ?? []).join(', ');
		enabled = starter?.enabled ?? true;
		isDefault = starter?.isDefault ?? false;
	});

	const canSave = $derived(title.trim() !== '' && content.trim() !== '' && !busy);

	function save(): void {
		if (!canSave) return;
		onsave({
			title: title.trim(),
			content,
			locale,
			tags: tags
				.split(',')
				.map((tag) => tag.trim())
				.filter((tag) => tag !== ''),
			enabled,
			isDefault,
		});
	}
</script>

<Dialog
	bind:open
	title={isEdit ? t('page.starters.editTitle') : t('page.starters.newTitle')}
	description={t('page.starters.editDesc')}
>
	<div class="space-y-3">
		<Input bind:value={title} label={t('page.starters.titleLabel')} size="sm" />
		<Textarea
			bind:value={content}
			label={t('page.starters.contentLabel')}
			rows={4}
			placeholder={t('page.starters.contentPlaceholder')}
		/>
		<div class="grid grid-cols-2 gap-3">
			<Select
				bind:value={locale}
				label={t('page.starters.localeLabel')}
				size="sm"
				options={[
					{ value: 'en', label: 'English' },
					{ value: 'zh', label: '中文' },
				]}
			/>
			<Input bind:value={tags} label={t('page.starters.tagsLabel')} size="sm" />
		</div>
		<div class="space-y-2">
			<Switch bind:checked={enabled} label={t('page.starters.enabledLabel')} />
			<Switch bind:checked={isDefault} label={t('page.starters.defaultLabel')} />
		</div>
		{#if error}
			<ErrorState title={t('page.starters.saveFailed')} description={error} />
		{/if}
	</div>
	{#snippet footer()}
		<div class="flex justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (open = false)}>
				{t('common.cancel')}
			</Button>
			<Button size="sm" disabled={!canSave} onclick={save}>
				{isEdit ? t('common.save') : t('common.create')}
			</Button>
		</div>
	{/snippet}
</Dialog>
