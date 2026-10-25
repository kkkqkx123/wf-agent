<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import JsonEditor from '#lib/ui/components/JsonEditor.svelte';
	import UnsavedChangesDialog from '#lib/ui/components/UnsavedChangesDialog.svelte';
	import { jsonErrorLine } from '#lib/services/templates';
	import type { TemplateKind } from '#lib/types/models';
	import { t } from '#lib/i18n';

	interface Props {
		open: boolean;
		kind: TemplateKind;
		text: string;
		error: string | null;
		busy: boolean;
		onimport: () => void;
		ondiscard?: () => void;
	}

	let {
		open = $bindable(false),
		kind = $bindable('node'),
		text = $bindable(''),
		error,
		busy,
		onimport,
		ondiscard,
	}: Props = $props();

	let editor = $state<{ scrollToLine: (line: number) => void } | null>(null);
	let confirmOpen = $state(false);

	const errorLine = $derived(error ? jsonErrorLine(text, error) : null);
	const hasText = $derived(text.trim() !== '');

	// Unsent import text blocks implicit closes; the underlying dialog
	// already flipped open before onclose runs, so guarded closes reopen
	// synchronously within the same update batch.
	function requestClose(): void {
		if (hasText) {
			open = true;
			confirmOpen = true;
			return;
		}
		open = false;
	}

	function discardText(): void {
		confirmOpen = false;
		text = '';
		open = false;
		ondiscard?.();
	}
</script>

<Dialog
	bind:open
	title={t('domain.templateImport.title')}
	description={t('domain.templateImport.description')}
	onclose={requestClose}
>
	<Select
		bind:value={kind}
		options={[
			{ value: 'node', label: t('domain.templateImport.node') },
			{ value: 'trigger', label: t('domain.templateImport.trigger') },
			{ value: 'agent', label: t('domain.templateImport.agent') },
			{ value: 'workflow', label: t('domain.templateImport.workflow') },
		]}
		size="sm"
		placeholder={t('domain.templateImport.kind')}
		class="mb-2 w-40"
	/>
	<JsonEditor
		bind:this={editor}
		bind:value={text}
		{errorLine}
		placeholder={'{\n  "id": "my-template",\n  …\n}'}
		label={t('domain.templateImport.jsonLabel')}
		minHeight="12rem"
	/>
	{#if error}
		<div
			class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
		>
			<p class="text-caption text-destructive">
				{t('domain.templateEdit.syntaxError', {
					line: errorLine,
					error: error,
				})}
			</p>
			{#if errorLine}
				<Button variant="ghost" size="sm" onclick={() => editor?.scrollToLine(errorLine ?? 1)}>
					{t('domain.templateEdit.goToLine')}
				</Button>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={requestClose}>{t('common.cancel')}</Button>
			<Button size="sm" disabled={busy} onclick={() => onimport()}>
				{busy ? t('domain.templateImport.importing') : t('domain.templateImport.import')}
			</Button>
		</div>
	{/snippet}
</Dialog>

<UnsavedChangesDialog
	bind:open={confirmOpen}
	title={t('domain.templateImport.discardTitle')}
	description={t('domain.templateImport.discardDescription')}
	discardLabel={t('domain.templateImport.discardLabel')}
	{busy}
	ondiscard={discardText}
/>
