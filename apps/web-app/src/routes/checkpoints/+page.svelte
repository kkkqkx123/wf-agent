<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';

	import {
		listCheckpoints,
		getApprovalRequests,
		createFileCheckpoint,
	} from '#lib/services/checkpoints';
	import type { Approval, Checkpoint } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace } from '#lib/stores/fileWorkspace.svelte';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import type { CheckpointDomain } from '#lib/stores/checkpoints.svelte';
	import { t } from '#lib/i18n';

	const TABS = [
		{ id: 'chain', label: t('page.checkpoints.tabChain') },
		{ id: 'files', label: t('page.checkpoints.tabFiles') },
		{ id: 'approvals', label: t('page.checkpoints.tabApprovals') },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'chain',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, { tab: tab === 'chain' ? '' : tab });
	});

	// ── chain tab ──
	let chain = $state<Checkpoint[]>([]);
	// ── approvals tab ──
	let approvals = $state<Approval[]>([]);

	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let chainDirty = $state(false);
	let approvalsDirty = $state(false);
	let stopLive: (() => void) | null = null;

	/** Drop decided rows from the local list before the refetch lands. */
	function removeApprovals(ids: string[]): void {
		const decided = new Set(ids);
		approvals = approvals.filter((approval) => !decided.has(approval.id));
	}

	// ── header dialog ──
	let createOpen = $state(false);

	/** Segment sources already pulled, so a tab loads once. */
	let seenChain = $state(false);
	let seenApprovals = $state(false);

	onMount(() => {
		stopLive = checkpointStore.subscribeLive((domain: CheckpointDomain) => {
			// The files tab follows `filesVersion` itself inside FilesPanel.
			if (domain === 'files') return;
			if (loading) {
				if (domain === 'chain') chainDirty = true;
				else approvalsDirty = true;
				return;
			}
			void reloadDomain(domain);
		});
	});

	onDestroy(() => {
		stopLive?.();
		stopLive = null;
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		// The files tab owns its segment inside FilesSection; it follows the
		// store version itself.
		if (current === 'files') return;
		const pending = current === 'chain' ? !seenChain : !seenApprovals;
		if (!pending) return;
		loading = true;
		loadError = null;
		try {
			if (current === 'chain') {
				seenChain = true;
				chain = (await listCheckpoints({ limit: 200 })).items;
			} else {
				seenApprovals = true;
				approvals = await getApprovalRequests();
			}
		} catch (e) {
			console.error('Failed to load checkpoints segment:', e);
			loadError = e instanceof Error ? e.message : t('page.checkpoints.requestFailed');
		} finally {
			loading = false;
			if (chainDirty || approvalsDirty) {
				const pending: CheckpointDomain | null = chainDirty
					? 'chain'
					: approvalsDirty
						? 'approvals'
						: null;
				chainDirty = false;
				approvalsDirty = false;
				if (pending) void reloadDomain(pending);
			}
		}
	}

	/** Refresh one domain tab after a live event or local mutation. A
	 * hidden tab is only invalidated so its next visit refetches. */
	async function reloadDomain(domain: CheckpointDomain): Promise<void> {
		if (domain === 'files') {
			// FilesPanel re-pulls through the `filesVersion` bump.
			return;
		}
		if (domain === 'chain') {
			if (tab !== 'chain') {
				seenChain = false;
				return;
			}
			seenChain = false;
			chain = [];
		} else {
			if (tab !== 'approvals') {
				seenApprovals = false;
				return;
			}
			seenApprovals = false;
			approvals = [];
		}
		await loadTab(tab);
	}

	async function reload(): Promise<void> {
		seenChain = false;
		seenApprovals = false;
		chain = [];
		approvals = [];
		// Bump the files domain so FilesPanel re-pulls its own segment too.
		checkpointStore.bump('files');
		await loadTab(tab);
	}

	async function runCreateFileCheckpoint(): Promise<void> {
		const actor = fileWorkspace.actor.trim() || 'manual';
		if (!actor) {
			toasts.error(t('page.checkpoints.enterActor'));
			return;
		}
		checkpointStore.beginAction('create');
		try {
			const created = await createFileCheckpoint(actor);
			createOpen = false;
			toasts.success(
				t('page.checkpoints.fileCheckpointCreated'),
				`${created.fileCount} files · ${created.id.slice(0, 10)}`,
			);
			checkpointStore.bump('files');
		} catch (e) {
			console.error('File checkpoint creation failed:', e);
			toasts.error(
				t('page.checkpoints.creationFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	function emptyCopyFor(current: string): {
		title: string;
		description: string;
	} {
		if (current === 'approvals') {
			return {
				title: t('page.checkpoints.noApprovals'),
				description:
					t('page.checkpoints.noApprovalsDesc'),
			};
		}
		return {
			title: t('page.checkpoints.noCheckpoints'),
			description:
				t('page.checkpoints.noCheckpointsDesc'),
		};
	}

	const segmentEmpty = $derived(
		tab === 'chain'
			? chain.length === 0
			: tab === 'files'
				? false
				: approvals.length === 0,
	);

	const segmentCopy = $derived(emptyCopyFor(tab));

	const liveLabel = $derived(
		checkpointStore.liveState === 'open'
			? t('page.checkpoints.live')
			: checkpointStore.liveState === 'reconnecting'
				? t('page.checkpoints.reconnecting')
				: t('page.checkpoints.offline'),
	);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={t('page.checkpoints.title')}
		description={t('page.checkpoints.description')}
	>
		{#snippet actions()}
			<Badge
				variant={checkpointStore.liveState === 'open' ? 'success' : 'neutral'}
			>
				{liveLabel}
			</Badge>
			<IconButton
				icon="refresh"
				label={t('common.refresh')}
				onclick={() => void reload()}
			/>
			<Button
				size="sm"
				disabled={checkpointStore.busy}
				onclick={() => {
					if (!fileWorkspace.actor) fileWorkspace.setActor('manual');
					createOpen = true;
				}}
			>
				<Icon name="plus" size={13} />
				{t('page.checkpoints.createCheckpoint')}
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		bind:value={tab}
		class="px-4"
		panelId="checkpoints-panel"
	/>

	<div
		id="checkpoints-panel"
		role="tabpanel"
		aria-label={t('page.checkpoints.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		<PageState
			{loading}
			error={loadError}
			empty={segmentEmpty}
			emptyTitle={segmentCopy.title}
			emptyDescription={segmentCopy.description}
			onretry={() => void reload()}
		>
			{#if tab === 'chain'}
				{#await import('#lib/components/domain/checkpoints/ChainSection.svelte') then module}
					<module.default {chain} />
				{/await}
			{:else if tab === 'files'}
				{#await import('#lib/components/domain/checkpoints/FilesSection.svelte') then module}
					<module.default />
				{/await}
			{:else}
				{#await import('#lib/components/domain/checkpoints/ApprovalsSection.svelte') then module}
					<module.default {approvals} onremove={removeApprovals} />
				{/await}
			{/if}
		</PageState>
	</div>
</div>

<Dialog
	bind:open={createOpen}
	title={t('page.checkpoints.createFileCheckpoint')}
	description={t('page.checkpoints.createFileCheckpointDesc')}
>
	<Input
		label={t('page.checkpoints.actor')}
		value={fileWorkspace.actor}
		oninput={(event: Event) =>
			fileWorkspace.setActor((event.target as HTMLInputElement).value)}
		placeholder="manual"
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (createOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy || !fileWorkspace.actor.trim()}
			onclick={() => void runCreateFileCheckpoint()}
		>
			Create
		</Button>
	{/snippet}
</Dialog>
