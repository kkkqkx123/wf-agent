<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import JsonEditor from '$lib/components/domain/JsonEditor.svelte';
	import UnsavedChangesDialog from '$lib/components/domain/UnsavedChangesDialog.svelte';
	import { jsonErrorLine } from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';

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
	title="Import template"
	description="Import any template kind from JSON text."
	onclose={requestClose}
>
	<Select
		bind:value={kind}
		options={[
			{ value: 'node', label: 'Node' },
			{ value: 'trigger', label: 'Trigger' },
			{ value: 'agent', label: 'Agent' },
			{ value: 'workflow', label: 'Workflow' },
		]}
		size="sm"
		placeholder="Kind"
		class="mb-2 w-40"
	/>
	<JsonEditor
		bind:this={editor}
		bind:value={text}
		{errorLine}
		placeholder={'{\n  "id": "my-template",\n  …\n}'}
		label="Import template JSON"
		minHeight="12rem"
	/>
	{#if error}
		<div
			class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
		>
			<p class="text-caption text-destructive">
				JSON syntax{#if errorLine}
					(line {errorLine}){/if}: {error}
			</p>
			{#if errorLine}
				<Button
					variant="ghost"
					size="sm"
					onclick={() => editor?.scrollToLine(errorLine ?? 1)}
				>
					Go to line
				</Button>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={requestClose}>Cancel</Button>
			<Button size="sm" disabled={busy} onclick={() => onimport()}>
				{busy ? 'Importing…' : 'Import'}
			</Button>
		</div>
	{/snippet}
</Dialog>

<UnsavedChangesDialog
	bind:open={confirmOpen}
	title="Discard import text?"
	description="The import text has not been imported yet. Closing now discards it."
	discardLabel="Discard import text"
	{busy}
	ondiscard={discardText}
/>
