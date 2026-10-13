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
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import ApprovalsPanel from '$lib/components/domain/checkpoints/ApprovalsPanel.svelte';
	import ChainPanel from '$lib/components/domain/checkpoints/ChainPanel.svelte';
	import FilesPanel from '$lib/components/domain/checkpoints/FilesPanel.svelte';
	import {
		listCheckpoints,
		getApprovalRequests,
		approveApproval,
		rejectApproval,
		deleteCheckpoint,
		resolveConflicts,
		createFileCheckpoint,
		restoreCheckpoint,
		resumeFromCheckpoint,
		type ConflictFile,
	} from '$lib/services/checkpoints';
	import type { Approval, Checkpoint } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

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
	let liveDirty = $state(false);
	let stopLive: (() => void) | null = null;

	// ── dialogs ──
	let createOpen = $state(false);
	let restoreOpen = $state(false);
	let restoreTarget = $state<Checkpoint | null>(null);
	let deleteOpen = $state(false);
	let deleteTarget = $state<Checkpoint | null>(null);
	let rejectOpen = $state(false);
	let rejectTarget = $state<Approval | null>(null);
	let rejectReason = $state('');
	let resolveOpen = $state(false);
	let resolveTarget = $state<ConflictFile | null>(null);
	let resolveFeature = $state('');
	let resolveContent = $state('');

	/** Segment sources already pulled, so a tab loads once. */
	let seenChain = $state(false);
	let seenApprovals = $state(false);

	onMount(() => {
		stopLive = checkpointStore.subscribeLive(() => {
			if (loading) {
				liveDirty = true;
				return;
			}
			void reloadCurrent();
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
		// The files tab owns its segment inside FilesPanel; it follows the
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
			if (liveDirty) {
				liveDirty = false;
				void reloadCurrent();
			}
		}
	}

	/** Refresh the visible tab after a live checkpoint event. */
	async function reloadCurrent(): Promise<void> {
		if (tab === 'chain') {
			seenChain = false;
			chain = [];
		} else if (tab === 'approvals') {
			seenApprovals = false;
			approvals = [];
		} else {
			// FilesPanel already re-pulled through the store version bump.
			return;
		}
		await loadTab(tab);
	}

	async function reload(): Promise<void> {
		seenChain = false;
		seenApprovals = false;
		chain = [];
		approvals = [];
		// Bump the version so FilesPanel re-pulls its own segment too.
		checkpointStore.bump();
		await loadTab(tab);
	}

	async function runCreateFileCheckpoint(): Promise<void> {
		const actor = checkpointStore.actor.trim() || 'manual';
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
			checkpointStore.bump();
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

	async function runRestore(): Promise<void> {
		const target = restoreTarget;
		if (!target) return;
		checkpointStore.beginAction('restore');
		try {
			await restoreCheckpoint(target);
			restoreOpen = false;
			restoreTarget = null;
			toasts.success(
				'Checkpoint restored',
				'Restore landed as a new commit; history was not rewritten.',
			);
			checkpointStore.bump();
		} catch (e) {
			console.error('Restore failed:', e);
			toasts.error(
				'Restore failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runResume(checkpoint: Checkpoint): Promise<void> {
		if (checkpoint.entityType === 'agent_loop') {
			toasts.error('Resume unavailable for agent-loop checkpoints');
			return;
		}
		checkpointStore.beginAction('resume');
		try {
			await resumeFromCheckpoint(checkpoint.id);
			toasts.success(
				'Resume requested',
				`Execution continues from ${checkpoint.id.slice(0, 10)}`,
			);
		} catch (e) {
			console.error('Resume failed:', e);
			toasts.error('Resume failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runDelete(): Promise<void> {
		const target = deleteTarget;
		if (!target) return;
		checkpointStore.beginAction('delete');
		try {
			await deleteCheckpoint(target.id);
			deleteOpen = false;
			deleteTarget = null;
			toasts.success('Checkpoint deleted');
			checkpointStore.bump();
		} catch (e) {
			console.error('Delete failed:', e);
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runResolveConflict(): Promise<void> {
		const target = resolveTarget;
		if (!target) return;
		const actor = checkpointStore.actor.trim();
		if (!actor) {
			toasts.error('Enter the resolving actor');
			return;
		}
		if (!resolveContent) {
			toasts.error('Paste the resolved file content');
			return;
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
				remaining === 0 ? 'Conflict fully resolved' : 'Resolution recorded',
				remaining === 0 ? target.path : `${remaining} file(s) still conflicted`,
			);
			checkpointStore.bump();
		} catch (e) {
			console.error('Conflict resolution failed:', e);
			toasts.error(
				'Resolution failed',
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
	}

	async function decideApproval(
		approval: Approval,
		granted: boolean,
	): Promise<void> {
		try {
			if (granted) {
				await approveApproval(approval.id);
				toasts.success('Approval granted');
			} else {
				rejectTarget = approval;
				rejectOpen = true;
				rejectReason = '';
				return;
			}
			checkpointStore.bump();
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error('Approval decision failed');
		}
	}

	async function runReject(): Promise<void> {
		const target = rejectTarget;
		if (!target) return;
		try {
			await rejectApproval(target.id, rejectReason || undefined);
			rejectOpen = false;
			rejectTarget = null;
			rejectReason = '';
			toasts.warning('Approval rejected');
			checkpointStore.bump();
		} catch (e) {
			console.error('Failed to reject approval:', e);
			toasts.error('Approval rejection failed');
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
					if (!checkpointStore.actor) checkpointStore.actor = 'manual';
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
				<ChainPanel
					{chain}
					busy={checkpointStore.busy}
					onrestore={(checkpoint: Checkpoint) => {
						restoreTarget = checkpoint;
						restoreOpen = true;
					}}
					onresume={(checkpoint: Checkpoint) => void runResume(checkpoint)}
					ondelete={(checkpoint: Checkpoint) => {
						deleteTarget = checkpoint;
						deleteOpen = true;
					}}
				/>
			{:else if tab === 'files'}
				<FilesPanel
					onresolve={(conflict: ConflictFile) => openResolve(conflict)}
				/>
			{:else}
				<ApprovalsPanel
					{approvals}
					ondecide={(approval: Approval, granted: boolean) =>
						void decideApproval(approval, granted)}
				/>
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
		bind:value={checkpointStore.actor}
		placeholder="manual"
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (createOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy || !checkpointStore.actor.trim()}
			onclick={() => void runCreateFileCheckpoint()}
		>
			Create
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={restoreOpen}
	title="Restore checkpoint"
	description="Restore lands as a new commit on top of history. Pointers are never moved and other branches are untouched."
	onclose={() => (restoreTarget = null)}
>
	{#if restoreTarget}
		<p class="text-caption">
			Restore <span class="font-mono">{restoreTarget.id}</span>
			({restoreTarget.kind}
			· {restoreTarget.actor}). Files covered by this checkpoint return to the
			recorded state; everything else stays as-is.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (restoreOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runRestore()}
		>
			Restore as new commit
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={deleteOpen}
	title="Delete checkpoint"
	description="Delete removes the checkpoint record. File history in the object store is reclaimed by garbage collection."
	onclose={() => (deleteTarget = null)}
>
	{#if deleteTarget}
		<p class="text-caption">
			Delete <span class="font-mono">{deleteTarget.id}</span>? This cannot be
			undone from this page.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (deleteOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runDelete()}
		>
			Delete
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={rejectOpen}
	title="Reject approval"
	description="Rejecting rolls the approval partition back to its baseline. Only the pending reference is removed; history is untouched."
	onclose={() => (rejectTarget = null)}
>
	<Textarea
		label="Reason (optional)"
		bind:value={rejectReason}
		placeholder="Why is this change unacceptable? What should change next?"
		rows={3}
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (rejectOpen = false)}>
			Cancel
		</Button>
		<Button size="sm" onclick={() => void runReject()}>Reject</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={resolveOpen}
	title="Resolve conflict"
	description="Commit the resolved file content as a child of the conflicted head. Files still carrying markers stay conflicted."
	onclose={() => (resolveTarget = null)}
>
	{#if resolveTarget}
		<p class="mb-2 text-caption">
			<span class="font-mono">{resolveTarget.path}</span> on
			<span class="font-mono">{resolveTarget.partition}</span>
		</p>
	{/if}
	<div class="grid gap-2 sm:grid-cols-2">
		<Input
			label="Actor"
			bind:value={checkpointStore.actor}
			placeholder="Resolving actor"
		/>
		<Input
			label="Feature"
			bind:value={resolveFeature}
			placeholder="Feature name for refs/wf/feat/<name>"
		/>
	</div>
	<div class="mt-2">
		<Textarea
			label="Resolved content"
			bind:value={resolveContent}
			placeholder="Paste the full resolved file content"
			rows={10}
		/>
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resolveOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy ||
				!checkpointStore.actor.trim() ||
				!resolveFeature.trim() ||
				!resolveContent}
			onclick={() => void runResolveConflict()}
		>
			Commit resolution
		</Button>
	{/snippet}
</Dialog>
