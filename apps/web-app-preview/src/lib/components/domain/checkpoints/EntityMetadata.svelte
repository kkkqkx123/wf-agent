<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import {
		getEntityMetadata,
		setEntityMetadata,
	} from '#lib/services/checkpoints';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '#lib/stores/checkpoints.svelte';
	import { t } from '#lib/i18n';

	let metadataEntityId = $state('');
	let metadataText = $state('');
	let note = $state<string | null>(null);

	async function load(): Promise<void> {
		const entity = metadataEntityId.trim();
		if (!entity) {
			toasts.error(t('domain.checkpoint.entityIdRequired'));
			return;
		}
		try {
			const metadata = await getEntityMetadata(entity);
			metadataText = JSON.stringify(metadata, null, 2);
			note = t('domain.checkpoint.loadedKeys', {
				count: Object.keys(metadata).length,
			});
		} catch (e) {
			console.error('Metadata load failed:', e);
			toasts.error(
				t('domain.checkpoint.metadataLoadFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function save(): Promise<void> {
		const entity = metadataEntityId.trim();
		if (!entity) {
			toasts.error(t('domain.checkpoint.entityIdRequired'));
			return;
		}
		let parsed: unknown;
		try {
			parsed = JSON.parse(metadataText || '{}');
		} catch (e) {
			toasts.error(
				t('domain.checkpoint.metadataInvalidJson'),
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		if (
			parsed === null ||
			typeof parsed !== 'object' ||
			Array.isArray(parsed)
		) {
			toasts.error(t('domain.checkpoint.metadataMustBeObject'));
			return;
		}
		checkpointStore.beginAction('metadata');
		try {
			await setEntityMetadata(entity, parsed as Record<string, unknown>);
			note = t('domain.checkpoint.metadataSavedNote');
			toasts.success(t('domain.checkpoint.metadataReplaced'), entity);
		} catch (e) {
			console.error('Metadata save failed:', e);
			toasts.error(
				t('domain.checkpoint.metadataSaveFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">
		{t('domain.checkpoint.entityMetadataTitle')}
	</h3>
	<div class="flex items-end gap-2">
		<div class="min-w-0 flex-1">
			<Input
				label={t('domain.checkpoint.entityId')}
				size="sm"
				value={metadataEntityId}
				oninput={(event: Event) =>
					(metadataEntityId = (event.target as HTMLInputElement).value)}
				placeholder="execution id"
			/>
		</div>
		<Button variant="outline" size="sm" onclick={() => void load()}>
			{t('domain.checkpoint.load')}
		</Button>
	</div>
	<Textarea
		label={t('domain.checkpoint.metadataLabel')}
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
			{t('domain.checkpoint.saveMetadata')}
		</Button>
		{#if note}
			<span class="text-micro text-muted-foreground">{note}</span>
		{/if}
	</div>
</section>
