<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import {
		submitForReview,
		mergeToFeature,
		mergeToMain,
		rollbackFeature,
		type PartitionView,
	} from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';
	import { formatRelativeTime } from '$lib/utils/format';

	let {
		partitions,
	}: {
		// Feature partitions double as the merge-target and rollback
		// candidate source; the list itself stays container-owned so the
		// partitions card and this card never fetch it twice.
		partitions: PartitionView[];
	} = $props();

	const FEATURE_PREFIX = 'refs/wf/feat/';
	const NEW_FEATURE = '__new__';

	// Existing feature names enumerated from partition refs; hand input
	// stays as the fallback for new names.
	const featurePartitions = $derived(
		partitions.filter((part) => part.name.startsWith(FEATURE_PREFIX)),
	);
	const featureNames = $derived(
		featurePartitions.map((part) => part.name.slice(FEATURE_PREFIX.length)),
	);

	let reviewFeature = $state('');
	let reviewFeaturesText = $state('');
	let rollbackHead = $state('');
	let mergeTargets = $state<string[]>([]);

	// Confirmation gates for the main-line mutations: merging into main and
	// appending a revert commit both rewrite shared history, so the button
	// opens a dialog that names the exact target before running.
	let mergeMainOpen = $state(false);
	let rollbackOpen = $state(false);

	const pendingMainFeatures = $derived([
		...mergeTargets,
		...parseExtraFeatures(reviewFeaturesText),
	]);

	const targetOptions = $derived([
		...featureNames.map((name) => ({ value: name, label: name })),
		{ value: NEW_FEATURE, label: 'New feature…' },
	]);
	const targetSelectValue = $derived(
		reviewFeature && !featureNames.includes(reviewFeature)
			? NEW_FEATURE
			: reviewFeature,
	);
	const rollbackOptions = $derived(
		featurePartitions.map((part) => ({
			value: part.currentSnapshot,
			label: `${part.name.slice(FEATURE_PREFIX.length)} · ${part.currentSnapshot.slice(0, 10)} · ${formatRelativeTime(new Date(part.updatedAt).toISOString())}`,
		})),
	);

	function parseExtraFeatures(text: string): string[] {
		return text
			.split(/[,\s]+/)
			.map((part) => part.trim())
			.filter((part) => part.length > 0);
	}

	async function runSubmitReview(): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor to submit for review');
			return;
		}
		workspace.beginAction('submit');
		try {
			const { reviewCommit } = await submitForReview(actor);
			toasts.success('Submitted for review', reviewCommit.slice(0, 10));
			store.bump('files');
			store.bump('approvals');
		} catch (e) {
			console.error('Submit for review failed:', e);
			toasts.error('Submit failed', e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runMergeToFeature(): Promise<void> {
		const actor = workspace.actor.trim();
		const feature = reviewFeature.trim();
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (!feature) {
			toasts.error('Enter a feature name for refs/wf/feat/<name>');
			return;
		}
		workspace.beginAction('merge-feature');
		try {
			const outcome = await mergeToFeature(actor, feature);
			if (outcome.conflictFiles.length > 0) {
				toasts.warning(
					'Merge has conflicts',
					outcome.conflictFiles.slice(0, 3).join(', '),
				);
			} else {
				toasts.success('Merged to feature', outcome.commitId.slice(0, 10));
			}
			store.bump('files');
		} catch (e) {
			console.error('Merge to feature failed:', e);
			toasts.error('Merge failed', e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runMergeToMain(): Promise<void> {
		const actor = workspace.actor.trim();
		const features = [
			...mergeTargets,
			...parseExtraFeatures(reviewFeaturesText),
		];
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (features.length === 0) {
			toasts.error('Enter at least one feature name');
			return;
		}
		workspace.beginAction('merge-main');
		try {
			const outcome = await mergeToMain(actor, features);
			mergeMainOpen = false;
			if (outcome.conflictFiles.length > 0) {
				toasts.warning(
					'Merge to main has conflicts',
					outcome.conflictFiles.slice(0, 3).join(', '),
				);
			} else {
				toasts.success('Merged to main', outcome.checkpointId.slice(0, 10));
			}
			store.bump('files');
		} catch (e) {
			console.error('Merge to main failed:', e);
			toasts.error(
				'Merge to main failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}

	async function runRollbackMergedFeature(): Promise<void> {
		const actor = workspace.actor.trim();
		const head = rollbackHead.trim();
		if (!actor) {
			toasts.error('Enter an actor for rollback');
			return;
		}
		if (!head) {
			toasts.error('Enter the merged feature head commit to revert');
			return;
		}
		workspace.beginAction('rollback-feature');
		try {
			const { revertCommit } = await rollbackFeature(actor, head);
			rollbackOpen = false;
			toasts.success('Feature rolled back', revertCommit.slice(0, 10));
			store.bump('files');
		} catch (e) {
			console.error('Feature rollback failed:', e);
			toasts.error(
				'Rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}
</script>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<Card
		title="Review & merge"
		description="Submit edit lines, merge features, roll back merged features"
	>
		<div class="grid gap-2 sm:grid-cols-2">
			<Input
				label="Actor"
				size="sm"
				value={workspace.actor}
				oninput={(event: Event) =>
					workspace.setActor((event.target as HTMLInputElement).value)}
				placeholder="Actor partition"
			/>
			{#if featureNames.length > 0}
				<Select
					label="Feature"
					size="sm"
					value={targetSelectValue}
					options={targetOptions}
					placeholder="Pick a feature"
					onchange={(value) => {
						reviewFeature = value === NEW_FEATURE ? '' : value;
					}}
				/>
			{/if}
		</div>
		{#if featureNames.length === 0 || targetSelectValue === NEW_FEATURE}
			<div class="mt-2">
				<Input
					label={featureNames.length === 0
						? 'Feature (no existing features yet)'
						: 'New feature name'}
					size="sm"
					value={reviewFeature}
					oninput={(event: Event) =>
						(reviewFeature = (event.target as HTMLInputElement).value)}
					placeholder="Feature name for refs/wf/feat/<name>"
				/>
			</div>
		{/if}
		{#if featureNames.length > 0}
			<fieldset class="mt-2">
				<legend class="text-caption text-muted-foreground">
					Features for main
				</legend>
				<div class="mt-1 flex flex-wrap gap-x-3 gap-y-1">
					{#each featureNames as name (name)}
						<label class="flex items-center gap-1.5 text-caption">
							<input
								type="checkbox"
								value={name}
								checked={mergeTargets.includes(name)}
								onchange={(event: Event) => {
									const checked = (event.target as HTMLInputElement).checked;
									mergeTargets = checked
										? [...mergeTargets, name]
										: mergeTargets.filter((item) => item !== name);
								}}
								class="accent-primary"
							/>
							<span class="font-mono">{name}</span>
						</label>
					{/each}
				</div>
			</fieldset>
		{/if}
		<div class="mt-2">
			<Input
				label={featureNames.length > 0
					? 'Extra features for main (comma or space separated)'
					: 'Features for main (comma or space separated)'}
				size="sm"
				value={reviewFeaturesText}
				oninput={(event: Event) =>
					(reviewFeaturesText = (event.target as HTMLInputElement).value)}
				placeholder="demo, next"
			/>
		</div>
		<div class="mt-2 flex flex-wrap gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={workspace.busy}
				onclick={() => void runSubmitReview()}
			>
				Submit for review
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={workspace.busy}
				onclick={() => void runMergeToFeature()}
			>
				Merge to feature
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={workspace.busy}
				onclick={() => (mergeMainOpen = true)}
			>
				Merge to main
			</Button>
		</div>
	</Card>

	<Card
		title="Feature rollback"
		description="Append a revert commit; other merged features stay untouched"
	>
		{#if rollbackOptions.length > 0}
			<Select
				label="Merged feature head"
				size="sm"
				value={rollbackHead}
				options={rollbackOptions}
				placeholder="Pick a feature head to revert"
				onchange={(value) => (rollbackHead = value)}
			/>
		{/if}
		<div class="mt-2">
			<Input
				label={rollbackOptions.length > 0
					? 'Head commit (prefilled from selection, editable)'
					: 'Merged feature head commit'}
				size="sm"
				value={rollbackHead}
				oninput={(event: Event) =>
					(rollbackHead = (event.target as HTMLInputElement).value)}
				placeholder="Merge commit id to revert"
			/>
		</div>
		<div class="mt-2">
			<Button
				variant="outline"
				size="sm"
				disabled={workspace.busy || !rollbackHead.trim()}
				onclick={() => (rollbackOpen = true)}
			>
				Roll back feature
			</Button>
		</div>
	</Card>
</div>

<Dialog
	bind:open={mergeMainOpen}
	title="Merge into main"
	description="Each feature is merged into main in order with its own merge commit. Main is the shared line; confirm the feature order before continuing."
	onclose={() => (mergeMainOpen = false)}
>
	<p class="text-caption">
		Merging into main:
		<span class="font-mono">{pendingMainFeatures.join(', ') || '—'}</span>
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (mergeMainOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={workspace.busy || pendingMainFeatures.length === 0}
			onclick={() => void runMergeToMain()}
		>
			Merge to main
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={rollbackOpen}
	title="Roll back merged feature"
	description="Appends a revert commit; other merged features are left untouched."
	onclose={() => (rollbackOpen = false)}
>
	<p class="text-caption">
		Revert <span class="font-mono">{rollbackHead.trim()}</span> as a new commit on
		main.
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (rollbackOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={workspace.busy || !rollbackHead.trim()}
			onclick={() => void runRollbackMergedFeature()}
		>
			Roll back
		</Button>
	{/snippet}
</Dialog>
