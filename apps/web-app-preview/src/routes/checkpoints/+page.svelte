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
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import ApprovalsSection from '$lib/components/domain/checkpoints/ApprovalsSection.svelte';
	import ChainSection from '$lib/components/domain/checkpoints/ChainSection.svelte';
	import FilesSection from '$lib/components/domain/checkpoints/FilesSection.svelte';
	import {
		listCheckpoints,
		getApprovalRequests,
		createFileCheckpoint,
	} from '$lib/services/checkpoints';
	import type { Approval, Checkpoint } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace } from '$lib/stores/fileWorkspace.svelte';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import type { CheckpointDomain } from '$lib/stores/checkpoints.svelte';

	const TABS = [
		{ id: 'chain', label: 'Checkpoint chain' },
		{ id: 'files', label: 'File workspace' },
		{ id: 'approvals', label: 'Approvals' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'chain',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'chain' ? '' : tab });
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
			loadError = e instanceof Error ? e.message : 'Checkpoint request failed';
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
			toasts.error('Enter an actor for the checkpoint');
			return;
		}
		checkpointStore.beginAction('create');
		try {
			const created = await createFileCheckpoint(actor);
			createOpen = false;
			toasts.success(
				'File checkpoint created',
				`${created.fileCount} files · ${created.id.slice(0, 10)}`,
			);
			checkpointStore.bump('files');
		} catch (e) {
			console.error('File checkpoint creation failed:', e);
			toasts.error(
				'Checkpoint creation failed',
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
				title: 'No approval requests',
				description:
					'Pending approvals appear here when a run asks for confirmation.',
			};
		}
		return {
			title: 'No checkpoints',
			description:
				'Checkpoints appear here once a run records a restorable state.',
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
			? 'Live'
			: checkpointStore.liveState === 'reconnecting'
				? 'Reconnecting'
				: 'Offline',
	);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Checkpoints & files"
		description="Execution checkpoint chains, file workspace changes and pending approvals."
	>
		{#snippet actions()}
			<Badge
				variant={checkpointStore.liveState === 'open' ? 'success' : 'neutral'}
			>
				{liveLabel}
			</Badge>
			<IconButton
				icon="refresh"
				label="Refresh"
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
				Create checkpoint
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
		aria-label="Checkpoint sections"
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
				<ChainSection {chain} />
			{:else if tab === 'files'}
				<FilesSection />
			{:else}
				<ApprovalsSection {approvals} onremove={removeApprovals} />
			{/if}
		</PageState>
	</div>
</div>

<Dialog
	bind:open={createOpen}
	title="Create file checkpoint"
	description="Capture the full workspace for one actor."
>
	<Input
		label="Actor"
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
