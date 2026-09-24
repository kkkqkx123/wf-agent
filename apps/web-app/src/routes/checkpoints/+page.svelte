<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import DiffView from '$lib/components/domain/DiffView.svelte';
	import type { DiffLine } from '$lib/components/domain/DiffView.svelte';
	import { listCheckpoints } from '$lib/services/checkpoints';
	import type { Checkpoint, FileChange, Approval } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatBytes,
		formatDateTime,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	const TABS = [
		{ id: 'chain', label: 'Checkpoint chain' },
		{ id: 'files', label: 'File workspace' },
		{ id: 'approvals', label: 'Approvals' },
	];

	let tab = $state('chain');
// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let loading = $state(true);
	let checkpoints = $state<Checkpoint[]>([]);
	const fileChanges = $state<FileChange[]>([]);
	const approvals = $state<Approval[]>([]);

	async function loadAll() {
		loading = true;
		try {
			const page = await listCheckpoints({ limit: 50 });
			checkpoints = page.items;
		} finally {
			loading = false;
		}
	}

	onMount(loadAll);

	const CHANGE_TONE: Record<string, string> = {
		added: 'text-success',
		modified: 'text-info',
		renamed: 'text-warning',
		deleted: 'text-destructive',
	};

	const SAMPLE_DIFF: DiffLine[] = [
		{ type: 'meta', text: 'crates/checkpoint/src/restore_coordinator.rs' },
		{
			type: 'context',
			text: 'pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
		},
		{ type: 'del', text: '    if self.branch_exists(target)? {' },
		{
			type: 'del',
			text: '        return Err(Error::BranchConflict(target.clone()));',
		},
		{ type: 'del', text: '    }' },
		{
			type: 'add',
			text: '    if let Some(existing) = self.find_snapshot(target)? {',
		},
		{ type: 'add', text: '        return Ok(existing);' },
		{ type: 'add', text: '    }' },
		{ type: 'context', text: '    self.write_snapshot(target)' },
		{ type: 'context', text: '}' },
	];
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
				onclick={loadAll}
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
							<li
								class="flex items-start justify-between gap-3 py-2 first:pt-0"
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
											CHANGE_TONE[change.changeType] ?? 'text-muted-foreground',
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
							</li>
						{/each}
					</ul>
				</Card>

				<div class="space-y-3">
					<DiffView lines={SAMPLE_DIFF} title="Selected delta" />
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
