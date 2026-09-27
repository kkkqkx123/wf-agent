<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import DiffView from '$lib/components/domain/DiffView.svelte';
	import {
		listCheckpoints,
		getFileChanges,
		getApprovalRequests,
		getStagedDiffs,
		type FileDiff,
	} from '$lib/services/checkpoints';
	import type { Approval, Checkpoint, FileChange } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatBytes,
		formatDateTime,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';
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
	let checkpoints = $state<Checkpoint[]>([]);
	let fileChanges = $state<FileChange[]>([]);
	let approvals = $state<Approval[]>([]);
	let selectedChangeId = $state<string | null>(null);
	let stagedDiff = $state<FileDiff | null>(null);
	let diffNote = $state<string | null>(null);
	let diffLoading = $state(false);

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		try {
			const [chain, files, pending] = await Promise.all([
				listCheckpoints({ limit: 200 }),
				getFileChanges(),
				getApprovalRequests(),
			]);
			checkpoints = chain.items;
			fileChanges = files;
			approvals = pending;
		} catch (e) {
			console.error('Failed to load checkpoints:', e);
		}
	}

	const CHANGE_TONE: Record<string, string> = {
		added: 'text-success',
		modified: 'text-info',
		renamed: 'text-warning',
		deleted: 'text-destructive',
	};

	async function selectChange(change: FileChange): Promise<void> {
		selectedChangeId = change.id;
		stagedDiff = null;
		diffNote = null;
		if (!change.actor) {
			diffNote = 'No actor recorded for this change';
			return;
		}
		diffLoading = true;
		try {
			const diffs = await getStagedDiffs(change.actor);
			const match = diffs.find((diff) => diff.path === change.path) ?? null;
			if (!match) {
				diffNote = 'No staged diff for this path';
				return;
			}
			stagedDiff = match;
		} catch (e) {
			console.error('Failed to load staged diff:', e);
			diffNote = 'Staged diff unavailable';
			toasts.error('Staged diff unavailable');
		} finally {
			diffLoading = false;
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Checkpoints & files"
		description="Execution checkpoint chains, file workspace changes and pending approvals."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button
				size="sm"
				onclick={() => toasts.success('Checkpoint request queued')}
			>
				<Icon name="plus" size={13} />
				Create checkpoint
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'chain'}
			<div class="space-y-2">
				{#each checkpoints as checkpoint (checkpoint.id)}
					<Card title="{checkpoint.kind} checkpoint · #{checkpoint.sequence}">
						{#snippet actions()}
							<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
								{checkpoint.restorable ? 'restorable' : 'locked'}
							</Badge>
						{/snippet}
						<div
							class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground"
						>
							<span class="font-mono">{checkpoint.id}</span>
							<span>{checkpoint.actor}</span>
							<span class="tabular-nums"
								>{formatBytes(checkpoint.sizeBytes)}</span
							>
							<span>{formatRelativeTime(checkpoint.createdAt)}</span>
						</div>
						<p class="mt-1.5 text-body">{checkpoint.note}</p>
						{#snippet footer()}
							<div class="flex flex-wrap items-center gap-2">
								<Button
									size="sm"
									disabled={!checkpoint.restorable}
									onclick={() => toasts.info('Restore queued')}
								>
									Restore
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.info('Resume queued')}
								>
									Resume from here
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.error('Delete requires confirmation')}
								>
									Delete
								</Button>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{:else if tab === 'files'}
			<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
				<Card
					title="Changes"
					description="Recent file deltas grouped by session"
				>
					<ul class="divide-y divide-border">
						{#each fileChanges as change (change.id)}
							<li class="py-2 first:pt-0">
								<button
									type="button"
									onclick={() => void selectChange(change)}
									aria-pressed={selectedChangeId === change.id}
									class={cn(
										'flex w-full items-start justify-between gap-3 rounded-md px-2 py-1 text-left transition-colors hover:bg-accent/50',
										selectedChangeId === change.id && 'bg-accent/70',
									)}
								>
									<div class="min-w-0">
										<p class="truncate font-mono text-caption">{change.path}</p>
										<p class="mt-0.5 text-micro text-muted-foreground">
											{change.actor} · {change.session} · {formatRelativeTime(
												change.at,
											)}
										</p>
									</div>
									<div class="flex shrink-0 items-center gap-2">
										<span
											class={cn(
												'text-micro',
												CHANGE_TONE[change.changeType] ??
													'text-muted-foreground',
											)}
										>
											{change.changeType}
										</span>
										<span class="text-micro tabular-nums text-success"
											>+{change.additions}</span
										>
										<span class="text-micro tabular-nums text-destructive"
											>-{change.deletions}</span
										>
									</div>
								</button>
							</li>
						{/each}
					</ul>
				</Card>

				<div class="space-y-3">
					{#if diffLoading}
						<Card title="Selected delta">
							<p class="text-caption text-muted-foreground">
								Loading staged diff…
							</p>
						</Card>
					{:else if stagedDiff && !stagedDiff.binary}
						<DiffView
							lines={stagedDiff.lines}
							title={stagedDiff.truncated
								? `${stagedDiff.path} · truncated`
								: stagedDiff.path}
						/>
					{:else if stagedDiff?.binary}
						<Card title={stagedDiff.path}>
							<p class="text-caption text-muted-foreground">
								Binary content has no text diff
							</p>
						</Card>
					{:else}
						<Card title="Selected delta">
							<p class="text-caption text-muted-foreground">
								{diffNote ?? 'Select a change to preview its staged diff'}
							</p>
						</Card>
					{/if}
					<Card title="Session actions">
						<div class="flex flex-wrap gap-2">
							<Button
								variant="outline"
								size="sm"
								onclick={() => toasts.info('Rollback queued')}
							>
								<Icon name="history" size={13} />
								Rollback session
							</Button>
							<Button
								variant="outline"
								size="sm"
								onclick={() => toasts.info('Undo queued')}
							>
								<Icon name="arrow-left" size={13} />
								Undo
							</Button>
							<Button
								variant="outline"
								size="sm"
								onclick={() => toasts.info('Metadata rebuild queued')}
							>
								<Icon name="database" size={13} />
								Rebuild metadata
							</Button>
						</div>
					</Card>
				</div>
			</div>
		{:else}
			<div class="space-y-2">
				{#each approvals as approval (approval.id)}
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
							{#if approval.status === 'pending'}
								<div class="flex items-center gap-2">
									<Button
										size="sm"
										onclick={() => toasts.success('Approval granted')}
									>
										<Icon name="check" size={13} />
										Approve
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() => toasts.warning('Approval rejected')}
									>
										<Icon name="x" size={13} />
										Reject
									</Button>
								</div>
							{:else}
								<span class="text-caption text-muted-foreground">
									Resolved · {approval.status}
								</span>
							{/if}
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>
