<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import DiffView from '$lib/components/domain/DiffView.svelte';
	import type { DiffLine } from '$lib/components/domain/DiffView.svelte';
	import {
		listCheckpoints,
		listPendingApprovals,
		approveChanges,
		rejectChanges,
		restoreCheckpoint,
		resumeFromCheckpoint,
		deleteCheckpoint,
		listFileActors,
		listStagedChanges,
		listEditSessions,
		undoEdit,
		redoEdit,
		rollbackSession,
	} from '$lib/services/checkpoints';
	import type {
		Checkpoint,
		EditSession,
		FileChange,
		Approval,
	} from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatBytes,
		formatDateTime,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'chain', label: 'Checkpoint chain' },
		{ id: 'files', label: 'File workspace' },
		{ id: 'approvals', label: 'Approvals' },
	];

	const initial = parseListParams(page.url);
	let tab = $state(initial.tab ?? 'chain');

	// Workspace reads are keyed by actor partition, so one has to be chosen
	// before any change list exists.
	let actorId = $state('');
	let selectedPath = $state<string | null>(null);
	let selectedSessionId = $state('');

	const list = createCollection((params) => listCheckpoints(params));
	const approvals = createResource(() => listPendingApprovals());
	const actors = createResource(() => listFileActors());
	const sessions = createResource(() => listEditSessions());
	const stagedChanges = createResource<FileChange[]>(async () =>
		actorId ? listStagedChanges(actorId) : [],
	);

	const actorOptions = $derived(
		(actors.data ?? []).map((actor) => ({
			value: actor.actor,
			label: `${actor.actor} · ${actor.kind}`,
		})),
	);

	const sessionOptions = $derived(
		(sessions.data ?? []).map((session) => ({
			value: session.id,
			label: `${session.label} · ${formatRelativeTime(session.createdAt)}`,
		})),
	);

	const changes = $derived(stagedChanges.data ?? []);

	const selected = $derived(
		changes.find((change) => change.path === selectedPath) ??
			changes[0] ??
			null,
	);

	const diffLines = $derived(toDiffLines(selected?.diff ?? null));

	$effect(() => {
		if (!actorId && actors.data && actors.data.length > 0) {
			actorId = actors.data[0].actor;
		}
	});

	$effect(() => {
		if (!actorId) return;
		selectedPath = null;
		void stagedChanges.reload();
	});

	function refreshAll(): void {
		list.reload();
		approvals.reload();
		actors.reload();
		sessions.reload();
		if (actorId) stagedChanges.reload();
	}

	// ── checkpoint chain / workspace mutations, all confirm-gated ──

	type PendingAction =
		| { kind: 'restore' | 'resume' | 'delete'; checkpoint: Checkpoint }
		| { kind: 'rollback'; session: EditSession }
		| null;

	let pending = $state<PendingAction>(null);
	let confirming = $state(false);
	let busy = $state(false);

	function ask(action: Exclude<PendingAction, null>): void {
		pending = action;
		confirming = true;
	}

	function closeConfirm(): void {
		confirming = false;
		pending = null;
	}

	const confirmCopy = $derived.by(() => {
		if (!pending) {
			return { title: '', detail: '', verb: '' };
		}
		if (pending.kind === 'rollback') {
			return {
				title: 'Roll back edit session',
				detail: `Every change recorded by session ${pending.session.label} is undone in actor ${actorId}.`,
				verb: 'Roll back',
			};
		}
		if (pending.kind === 'delete') {
			return {
				title: 'Delete checkpoint',
				detail: `Checkpoint ${pending.checkpoint.id} is removed from storage and cannot be restored.`,
				verb: 'Delete',
			};
		}
		const target = pending.checkpoint;
		return pending.kind === 'restore'
			? {
					title: 'Restore checkpoint',
					detail: `${target.entityType} ${target.entityId} is rolled back to position ${target.chainPosition ?? '?'}.`,
					verb: 'Restore',
				}
			: {
					title: 'Resume from checkpoint',
					detail: `${target.entityType} ${target.entityId} restarts from position ${target.chainPosition ?? '?'} and re-runs the remaining nodes.`,
					verb: 'Resume',
				};
	});

	async function runPending(): Promise<void> {
		const action = pending;
		if (!action) return;
		busy = true;
		try {
			if (action.kind === 'rollback') {
				await rollbackSession(action.session.id, actorId);
				toasts.success(`Rolled back session ${action.session.label}`);
			} else {
				const target = action.checkpoint;
				if (action.kind === 'delete') {
					await deleteCheckpoint(target.id);
					toasts.success(`Deleted checkpoint ${target.id}`);
				} else if (action.kind === 'restore') {
					await restoreCheckpoint(target.id);
					toasts.success(`Restored checkpoint ${target.id}`);
				} else {
					await resumeFromCheckpoint(target.id);
					toasts.success(`Resumed ${target.entityType} ${target.entityId}`);
				}
			}
			const wasRollback = action.kind === 'rollback';
			closeConfirm();
			if (wasRollback) {
				await Promise.all([stagedChanges.reload(), sessions.reload()]);
			} else {
				await list.reload();
			}
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}

	async function step(direction: 'undo' | 'redo'): Promise<void> {
		if (!actorId) return;
		busy = true;
		try {
			if (direction === 'undo') await undoEdit(actorId);
			else await redoEdit(actorId);
			await stagedChanges.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : `${direction} failed`);
		} finally {
			busy = false;
		}
	}

	// ── approvals ──

	let decidedId = $state<string | null>(null);
	let rejecting = $state(false);

	async function decide(
		action: 'approve' | 'reject',
		approval: Approval,
	): Promise<void> {
		decidedId = approval.id;
		try {
			if (action === 'approve') await approveChanges(approval.id);
			else await rejectChanges(approval.id);
			toasts.success(
				action === 'approve'
					? `Approved changes from ${approval.requester}`
					: `Rejected changes from ${approval.requester}`,
			);
			await approvals.reload();
		} catch (e) {
			toasts.error(
				e instanceof Error ? e.message : 'Failed to record decision',
			);
		} finally {
			decidedId = null;
			rejecting = false;
		}
	}

	$effect(() => {
		gotoWithParams(page.url, {
			tab,
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void list.loadPages(Number(initial.page) || 1);
		void approvals.reload();
		void actors.reload();
		void sessions.reload();
	});

	const CHANGE_TONE: Record<string, string> = {
		added: 'text-success',
		modified: 'text-info',
		deleted: 'text-destructive',
	};

	/** Splits a unified diff into renderable lines, dropping file headers. */
	function toDiffLines(diff: string | null): DiffLine[] {
		if (diff === null) return [];
		const lines: DiffLine[] = [];
		for (const raw of diff.split('\n')) {
			if (
				raw === '' ||
				raw.startsWith('diff ') ||
				raw.startsWith('index ') ||
				raw.startsWith('--- ') ||
				raw.startsWith('+++ ')
			) {
				continue;
			}
			if (raw.startsWith('@@')) {
				lines.push({ type: 'meta', text: raw });
			} else if (raw.startsWith('+')) {
				lines.push({ type: 'add', text: raw.slice(1) });
			} else if (raw.startsWith('-')) {
				lines.push({ type: 'del', text: raw.slice(1) });
			} else {
				lines.push({
					type: 'context',
					text: raw.startsWith(' ') ? raw.slice(1) : raw,
				});
			}
		}
		return lines;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Checkpoints & files"
		description="Execution checkpoint chains, file workspace changes and pending approvals."
	>
		{#snippet actions()}
			<IconButton icon="refresh" label="Refresh" onclick={refreshAll} />
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'chain'}
			{#if list.loading && list.loaded === 0}
				<div class="space-y-2">
					{#each Array.from({ length: 4 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="120px" class="rounded-lg" />
					{/each}
				</div>
			{:else if list.error}
				<ErrorState
					title="Failed to load checkpoints"
					description={list.error}
					onretry={() => list.reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if list.loaded === 0}
				<EmptyState
					icon="history"
					title="No checkpoints yet"
					description="Checkpoint chains appear once executions produce snapshots."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-2">
					{#each list.items as checkpoint (checkpoint.id)}
						<Card
							title="{checkpoint.kind} · position {checkpoint.chainPosition ??
								'?'}"
						>
							{#snippet actions()}
								<StatusBadge status={checkpoint.status} size="sm" dot={false} />
							{/snippet}
							<div
								class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground"
							>
								<span class="font-mono">{checkpoint.id}</span>
								<span>{checkpoint.entityType} {checkpoint.entityId}</span>
								<span class="tabular-nums"
									>{formatBytes(checkpoint.sizeBytes)}</span
								>
								<span>{formatRelativeTime(checkpoint.createdAt)}</span>
							</div>
							{#if checkpoint.tags.length > 0}
								<div class="mt-1.5 flex flex-wrap gap-1.5">
									{#each checkpoint.tags as tag (tag)}
										<Badge variant="outline" size="sm">{tag}</Badge>
									{/each}
								</div>
							{/if}
							{#snippet footer()}
								<div class="flex flex-wrap items-center gap-2">
									<Button
										size="sm"
										disabled={busy}
										onclick={() => ask({ kind: 'restore', checkpoint })}
									>
										Restore
									</Button>
									<Button
										variant="ghost"
										size="sm"
										disabled={busy}
										onclick={() => ask({ kind: 'resume', checkpoint })}
									>
										Resume from here
									</Button>
									<Button
										variant="ghost"
										size="sm"
										disabled={busy}
										onclick={() => ask({ kind: 'delete', checkpoint })}
									>
										<Icon name="trash" size={13} />
										Delete
									</Button>
								</div>
							{/snippet}
						</Card>
					{/each}
				</div>
			{/if}
		{:else if tab === 'files'}
			{#if actors.loading && !actors.data}
				<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
					{#each Array.from({ length: 2 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="220px" class="rounded-lg" />
					{/each}
				</div>
			{:else if actors.error}
				<ErrorState
					title="Failed to load the file workspace"
					description={actors.error}
					onretry={() => actors.reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if !actorId}
				<EmptyState
					icon="folder"
					title="No actor partitions"
					description="File checkpointing records changes per actor; none have written yet."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
					<Card
						title="Changes vs staged"
						description="Files where this actor workspace differs from the staged partition."
					>
						{#snippet actions()}
							<Select
								size="sm"
								bind:value={actorId}
								options={actorOptions}
								placeholder="Actor"
							/>
						{/snippet}
						{#if stagedChanges.loading && !stagedChanges.data}
							<div class="space-y-2">
								{#each Array.from({ length: 4 }, (_, position) => position) as index (index)}
									<Skeleton shape="block" height="34px" class="rounded-md" />
								{/each}
							</div>
						{:else if stagedChanges.error}
							<ErrorState
								title="Failed to load staged changes"
								description={stagedChanges.error}
								onretry={() => stagedChanges.reload()}
							/>
						{:else if changes.length === 0}
							<p class="text-caption text-muted-foreground">
								Workspace matches the staged partition.
							</p>
						{:else}
							<ul class="divide-y divide-border">
								{#each changes as change (change.path)}
									<li>
										<button
											type="button"
											class="flex w-full items-center justify-between gap-3 py-2 text-left first:pt-0"
											onclick={() => (selectedPath = change.path)}
										>
											<span
												class={cn(
													'truncate font-mono text-caption',
													selected?.path === change.path &&
														'text-foreground font-medium',
												)}
											>
												{change.path}
											</span>
											<span class="flex shrink-0 items-center gap-2">
												<span
													class={cn(
														'text-micro',
														CHANGE_TONE[change.kind] ?? 'text-muted-foreground',
													)}
												>
													{change.kind}
												</span>
												<span class="text-micro tabular-nums text-success"
													>+{change.additions}</span
												>
												<span class="text-micro tabular-nums text-destructive"
													>-{change.deletions}</span
												>
											</span>
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					</Card>

					<div class="space-y-3">
						{#if selected}
							<DiffView
								lines={diffLines}
								title={selected.diff === null
									? `${selected.path} · binary`
									: selected.path}
							/>
						{:else}
							<Card title="Selected delta">
								<p class="text-caption text-muted-foreground">
									Select a file to inspect its unified diff.
								</p>
							</Card>
						{/if}
						<Card title="Workspace actions">
							<div class="flex flex-wrap items-end gap-2">
								<Select
									size="sm"
									bind:value={selectedSessionId}
									options={sessionOptions}
									placeholder="Edit session"
									class="min-w-56"
								/>
								<Button
									variant="outline"
									size="sm"
									disabled={!selectedSessionId || busy}
									onclick={() => {
										const session = (sessions.data ?? []).find(
											(item) => item.id === selectedSessionId,
										);
										if (session) ask({ kind: 'rollback', session });
									}}
								>
									<Icon name="history" size={13} />
									Roll back session
								</Button>
								<Button
									variant="outline"
									size="sm"
									disabled={busy}
									onclick={() => step('undo')}
								>
									<Icon name="arrow-left" size={13} />
									Undo
								</Button>
								<Button
									variant="outline"
									size="sm"
									disabled={busy}
									onclick={() => step('redo')}
								>
									<Icon name="arrow-right" size={13} />
									Redo
								</Button>
							</div>
							<p class="mt-2 text-micro text-muted-foreground">
								Undo, redo and rollback apply to
								<span class="font-mono text-foreground">{actorId}</span>.
							</p>
						</Card>
					</div>
				</div>
			{/if}
		{:else}
			{#if approvals.loading && !approvals.data}
				<div class="space-y-2">
					{#each Array.from({ length: 3 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="96px" class="rounded-lg" />
					{/each}
				</div>
			{:else if approvals.error}
				<ErrorState
					title="Failed to load approvals"
					description={approvals.error}
					onretry={() => approvals.reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if !approvals.data || approvals.data.length === 0}
				<EmptyState
					icon="check"
					title="No pending approvals"
					description="File changes awaiting review will appear here."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-2">
					{#each approvals.data as approval (approval.id)}
						<Card title={approval.title}>
							{#snippet actions()}
								<StatusBadge status={approval.status} size="sm" />
							{/snippet}
							<p class="text-caption">{approval.detail}</p>
							<div
								class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-micro text-muted-foreground"
							>
								<span>{approval.kind}</span>
								<span>{approval.requester}</span>
								<span>{formatDateTime(approval.requestedAt)}</span>
								<span class="font-mono">{approval.executionId}</span>
							</div>
							{#snippet footer()}
								<div class="flex items-center gap-2">
									<Button
										size="sm"
										disabled={decidedId === approval.id}
										onclick={() => decide('approve', approval)}
									>
										<Icon name="check" size={13} />
										Approve
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={decidedId === approval.id}
										onclick={() => {
											rejecting = true;
											decidedId = approval.id;
										}}
									>
										<Icon name="x" size={13} />
										Reject
									</Button>
								</div>
							{/snippet}
						</Card>
					{/each}
				</div>

				<Dialog
					bind:open={rejecting}
					title="Reject pending changes"
					description="Rejected file changes roll the workspace back to its baseline snapshot. This cannot be undone from here."
					onclose={() => (decidedId = null)}
				>
					<p class="text-caption text-muted-foreground">
						The approval for
						<span class="font-mono text-foreground">{decidedId}</span>
						will be rolled back to its baseline snapshot.
					</p>
					{#snippet footer()}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => (rejecting = false)}
						>
							Cancel
						</Button>
						<Button
							variant="destructive"
							size="sm"
							disabled={!decidedId}
							onclick={() => {
								const target = approvals.data?.find(
									(item) => item.id === decidedId,
								);
								if (target) void decide('reject', target);
							}}
						>
							Reject changes
						</Button>
					{/snippet}
				</Dialog>
			{/if}
		{/if}
	</div>
</div>

<Dialog
	bind:open={confirming}
	title={confirmCopy.title}
	description={confirmCopy.detail}
	onclose={closeConfirm}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={closeConfirm}>Cancel</Button>
		<Button
			variant="destructive"
			size="sm"
			disabled={!pending || busy}
			onclick={() => void runPending()}
		>
			{confirmCopy.verb}
		</Button>
	{/snippet}
</Dialog>
