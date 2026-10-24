<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import DefinitionDraftsPanel from '#lib/components/domain/DefinitionDraftsPanel.svelte';
	import DefinitionVersionsPanel from '#lib/components/domain/DefinitionVersionsPanel.svelte';
	import RunsPanel from '#lib/components/domain/RunsPanel.svelte';
	import {
		listDefinitionDrafts,
		listDefinitionVersions,
		promoteDefinitionDraft,
		validateDefinitionDraft,
	} from '#lib/services/definitions';
	import { EditLockStore } from '#lib/stores/edit-lock.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import type {
		DefinitionDraft,
		DefinitionKind,
		DefinitionVersion,
	} from '#lib/types/models';
	import { t } from '#lib/i18n';

	interface Props {
		/** The kind of definition the capabilities belong to. */
		kind: DefinitionKind;
		/** The definition the capabilities apply to. */
		definitionId: string;
	}

	let { kind, definitionId }: Props = $props();

	// The lease is bound to one kind of definition for the lifetime of the
	// panel; the kind never changes under a mounted panel.
	const lock = new EditLockStore(untrack(() => kind));

	let drafts = $state<DefinitionDraft[]>([]);
	let versions = $state<DefinitionVersion[]>([]);
	let loading = $state(true);
	let loadError = $state<string | null>(null);

	async function load(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			const [loadedDrafts, loadedVersions] = await Promise.all([
				listDefinitionDrafts(kind, definitionId),
				listDefinitionVersions(kind, definitionId),
			]);
			drafts = loadedDrafts;
			versions = loadedVersions;
		} catch (e) {
			loadError =
				e instanceof Error ? e.message : t('domain.definition.loadFailed');
		} finally {
			loading = false;
		}
	}

	async function promote(draftId: string): Promise<void> {
		try {
			const report = await promoteDefinitionDraft(kind, draftId);
			toasts.success(
				t('domain.definition.promoted', {
					passed: report.passCount,
					warnings: report.warningCount,
				}),
			);
			await load();
		} catch (e) {
			toasts.error(
				t('domain.definition.promoteFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function validate(draftId: string): Promise<void> {
		try {
			const issues = await validateDefinitionDraft(kind, draftId);
			drafts = drafts.map((draft) =>
				draft.id === draftId
					? { ...draft, valid: issues.length === 0, issues }
					: draft,
			);
			toasts.success(
				issues.length === 0
					? t('domain.definition.validatePassed')
					: t('domain.definition.validateIssues', { count: issues.length }),
			);
		} catch (e) {
			toasts.error(
				t('domain.definition.validateFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	$effect(() => {
		lock.watch(definitionId);
	});

	onMount(() => {
		void load();
	});
</script>

<div class="space-y-3">
	<div class="flex flex-wrap items-center gap-2 text-caption">
		{#if lock.held}
			<Badge variant="success">{t('domain.workflowEdit.lockHeld')}</Badge>
		{:else if lock.lockedByOther}
			<Badge variant="danger"
				>{t('domain.workflowEdit.lockedByOther', {
					holder: lock.displayHolder,
				})}</Badge
			>
		{:else}
			<Badge variant="outline">{t('domain.workflowEdit.unlocked')}</Badge>
		{/if}
		{#if lock.refreshError}
			<span class="text-micro text-muted-foreground"
				>{t('domain.workflowEdit.lockQueryFailed')}</span
			>
		{/if}
		{#if !lock.held}
			<Button size="sm" onclick={() => void lock.acquire()}>
				{t('domain.definition.acquireLock')}
			</Button>
		{/if}
		{#if lock.held}
			<Button variant="ghost" size="sm" onclick={() => void lock.release()}>
				{t('domain.definition.releaseLock')}
			</Button>
		{/if}
	</div>

	{#if loading}
		<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
	{:else if loadError}
		<ErrorState
			title={t('domain.definition.loadFailed')}
			description={loadError}
			onretry={() => void load()}
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<Card title={t('domain.definition.draftsTitle')}>
			<DefinitionDraftsPanel
				{kind}
				{drafts}
				onpromote={(draftId) => void promote(draftId)}
				onvalidate={(draftId) => void validate(draftId)}
			/>
		</Card>

		<DefinitionVersionsPanel
			{kind}
			{definitionId}
			{versions}
			nodes={[]}
			edges={[]}
			onchanged={() => void load()}
		/>

		<RunsPanel {kind} {definitionId} />
	{/if}
</div>
