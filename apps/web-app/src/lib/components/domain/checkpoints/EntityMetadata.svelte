<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import { getEntityMetadata, setEntityMetadata } from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';

	let metadataEntityId = $state('');
	let metadataText = $state('');
	let note = $state<string | null>(null);

	async function load(): Promise<void> {
		const entity = metadataEntityId.trim();
		if (!entity) {
			toasts.error('Enter an entity id');
			return;
		}
		try {
			const metadata = await getEntityMetadata(entity);
			metadataText = JSON.stringify(metadata, null, 2);
			note = `Loaded ${Object.keys(metadata).length} key(s)`;
		} catch (e) {
			console.error('Metadata load failed:', e);
			toasts.error(
				'Metadata load failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function save(): Promise<void> {
		const entity = metadataEntityId.trim();
		if (!entity) {
			toasts.error('Enter an entity id');
			return;
		}
		let parsed: unknown;
		try {
			parsed = JSON.parse(metadataText || '{}');
		} catch (e) {
			toasts.error(
				'Metadata is not valid JSON',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
			toasts.error('Metadata must be a JSON object');
			return;
		}
		checkpointStore.beginAction('metadata');
		try {
			await setEntityMetadata(entity, parsed as Record<string, unknown>);
			note = 'Metadata saved';
			toasts.success('Metadata replaced', entity);
		} catch (e) {
			console.error('Metadata save failed:', e);
			toasts.error(
				'Metadata save failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">Entity metadata</h3>
	<div class="flex items-end gap-2">
		<div class="min-w-0 flex-1">
			<Input
				label="Entity id"
				size="sm"
				value={metadataEntityId}
				oninput={(event: Event) =>
					(metadataEntityId = (event.target as HTMLInputElement).value)}
				placeholder="execution id"
			/>
		</div>
		<Button variant="outline" size="sm" onclick={() => void load()}>
			Load
		</Button>
	</div>
	<Textarea
		label="Metadata (JSON object)"
		bind:value={metadataText}
		rows={6}
		placeholder="metadata JSON"
	/>
	<div class="flex items-center gap-2">
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void save()}
		>
			Save metadata
		</Button>
		{#if note}
			<span class="text-micro text-muted-foreground">{note}</span>
		{/if}
	</div>
</section>
