<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Textarea from '#lib/ui/components/Textarea.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import FilesPanel from '#lib/components/domain/checkpoints/FilesPanel.svelte';
	import {
		getFileContent,
		resolveConflicts,
		type ConflictFile,
	} from '#lib/services/checkpoints';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace } from '#lib/stores/fileWorkspace.svelte';
	import { hasConflictMarkers } from '#lib/types/diff';
	import { t } from '#lib/i18n';

	// ── dialogs ──
	let resolveOpen = $state(false);
	let resolveTarget = $state<ConflictFile | null>(null);
	let resolveFeature = $state('');
	let resolveContent = $state('');
	let resolveLoading = $state(false);
	let resolveBinary = $state(false);
	let resolveNote = $state<string | null>(null);

	async function runResolveConflict(): Promise<void> {
		const target = resolveTarget;
		if (!target) return;
		const actor = fileWorkspace.actor.trim();
		if (!actor) {
			toasts.error(t('domain.checkpoint.actorRequiredResolve'));
			return;
		}
		if (!resolveContent) {
			toasts.error(t('domain.checkpoint.contentRequired'));
			return;
		}
		if (resolveBinary) {
			toasts.error(
				t('domain.checkpoint.binaryCannotResolve'),
				t('domain.checkpoint.useTerminal'),
			);
			return;
		}
		if (hasConflictMarkers(resolveContent)) {
			toasts.warning(
				t('domain.checkpoint.markersRemainTitle'),
				t('domain.checkpoint.markersRemainDetail'),
			);
		}
		checkpointStore.beginAction('resolve');
		try {
			const { remaining } = await resolveConflicts(actor, resolveFeature, {
				[target.path]: resolveContent,
			});
			resolveOpen = false;
			resolveTarget = null;
			resolveContent = '';
			toasts.success(
				remaining === 0
					? t('domain.checkpoint.fullyResolved')
					: t('domain.checkpoint.resolutionRecorded'),
				remaining === 0
					? target.path
					: t('domain.checkpoint.stillConflicted', { count: remaining }),
			);
			checkpointStore.bump('files');
		} catch (e) {
			console.error('Conflict resolution failed:', e);
			toasts.error(
				t('domain.checkpoint.resolutionFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	function openResolve(conflict: ConflictFile): void {
		resolveTarget = conflict;
		resolveOpen = true;
		// Feature refs look like refs/wf/feat/<name>; main conflicts resolve
		// against an explicit feature name.
		resolveFeature = conflict.partition.startsWith('refs/wf/feat/')
			? conflict.partition.slice('refs/wf/feat/'.length)
			: '';
		resolveContent = '';
		resolveBinary = false;
		resolveNote = null;
		resolveLoading = true;
		// The conflicted bytes (with markers) are read at the conflicted
		// merge commit without touching any workspace; the actor only
		// satisfies path validation.
		const actor = fileWorkspace.actor.trim() || 'manual';
		getFileContent(actor, conflict.path, conflict.snapshotId)
			.then((content) => {
				if (resolveTarget !== conflict) return;
				resolveBinary = content.isBinary;
				if (content.isBinary) {
					resolveNote = t('domain.checkpoint.binaryNote');
					return;
				}
				resolveContent = content.content ?? '';
				if (!resolveContent) {
					resolveNote = t('domain.checkpoint.emptyContent');
				} else if (content.truncated) {
					resolveNote = t('domain.checkpoint.truncatedNote');
				}
			})
			.catch((e) => {
				if (resolveTarget !== conflict) return;
				console.error('Failed to load conflicted content:', e);
				resolveNote =
					e instanceof Error
						? t('domain.checkpoint.contentUnavailable', { error: e.message })
						: t('domain.checkpoint.contentUnavailableError');
			})
			.finally(() => {
				if (resolveTarget === conflict) resolveLoading = false;
			});
	}

	const resolveHasMarkers = $derived(hasConflictMarkers(resolveContent));
</script>

<FilesPanel onresolve={(conflict: ConflictFile) => openResolve(conflict)} />

<Dialog
	bind:open={resolveOpen}
	title={t('domain.checkpoint.resolveTitle')}
	description={t('domain.checkpoint.resolveDescription')}
	onclose={() => (resolveTarget = null)}
>
	{#if resolveTarget}
		<p class="mb-2 text-caption">
			<span class="font-mono">{resolveTarget.path}</span>
			{t('domain.checkpoint.onPartition')}
			<span class="font-mono">{resolveTarget.partition}</span>
		</p>
	{/if}
	<div class="grid gap-2 sm:grid-cols-2">
		<Input
			label={t('domain.files.actor')}
			value={fileWorkspace.actor}
			oninput={(event: Event) =>
				fileWorkspace.setActor((event.target as HTMLInputElement).value)}
			placeholder={t('domain.checkpoint.resolvingActor')}
		/>
		<Input
			label={t('domain.files.feature')}
			bind:value={resolveFeature}
			placeholder={t('domain.files.featureNamePlaceholder')}
		/>
	</div>
	<div class="mt-2">
		{#if resolveLoading}
			<Skeleton lines={6} />
		{:else if resolveBinary}
			<p class="text-caption text-warning">
				{t('domain.checkpoint.binaryWarning')}
			</p>
		{:else}
			{#if resolveTarget && resolveTarget.regions.length > 0}
				<div
					class="mb-2 max-h-48 overflow-y-auto rounded-md border border-border"
				>
					{#each resolveTarget.regions as region, index (index)}
						<div class="border-b border-border p-2 last:border-b-0">
							<p class="text-micro text-muted-foreground">
								{t('domain.checkpoint.region', {
									number: index + 1,
									line: region.startLine,
								})}
							</p>
							<div class="mt-1 grid gap-2 sm:grid-cols-3">
								<div>
									<p class="text-micro text-muted-foreground">
										{t('domain.checkpoint.base')}
									</p>
									<pre
										class="whitespace-pre-wrap font-mono text-micro">{region.base.join(
											'\n',
										) || '∅'}</pre>
								</div>
								<div>
									<p class="text-micro text-muted-foreground">
										{t('domain.checkpoint.ours')}
									</p>
									<pre
										class="whitespace-pre-wrap font-mono text-micro">{region.ours.join(
											'\n',
										) || '∅'}</pre>
								</div>
								<div>
									<p class="text-micro text-muted-foreground">
										{t('domain.checkpoint.theirs')}
									</p>
									<pre
										class="whitespace-pre-wrap font-mono text-micro">{region.theirs.join(
											'\n',
										) || '∅'}</pre>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
			<Textarea
				label={t('domain.checkpoint.resolvedContent')}
				bind:value={resolveContent}
				placeholder={t('domain.checkpoint.resolvedPlaceholder')}
				rows={10}
			/>
		{/if}
		{#if resolveNote}
			<p class="mt-1 text-micro text-muted-foreground">{resolveNote}</p>
		{/if}
		{#if resolveHasMarkers && !resolveBinary}
			<p class="mt-1 text-micro text-warning">
				{t('domain.checkpoint.markersRemain')}
			</p>
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resolveOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy ||
				resolveLoading ||
				resolveBinary ||
				!fileWorkspace.actor.trim() ||
				!resolveFeature.trim() ||
				!resolveContent}
			onclick={() => void runResolveConflict()}
		>
			{t('domain.checkpoint.commitResolution')}
		</Button>
	{/snippet}
</Dialog>
