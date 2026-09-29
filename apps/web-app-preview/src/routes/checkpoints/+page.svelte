<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import { toneText } from '$lib/components/ui/variants';
	import type { StatusTone } from '$lib/utils/status';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import DiffView from '$lib/components/ui/DiffView.svelte';
	import JsonViewer from '$lib/components/ui/JsonViewer.svelte';
	import CursorPager from '$lib/components/ui/CursorPager.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import { downloadFile } from '$lib/api/client';
	import {
		listCheckpoints,
		getFileChangesPage,
		getApprovalRequests,
		getStagedDiffs,
		getDiffActors,
		getFileContent,
		getFileTree,
		getFileTimeline,
		approveApproval,
		rejectApproval,
		type FileContent,
		type FileDiff,
		type FileTimeline,
		type FileTree,
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
	let previewContent = $state<FileContent | null>(null);
	let previewTimeline = $state<FileTimeline | null>(null);
	let previewTree = $state<FileTree | null>(null);
	let changesHasMore = $state(false);
	let changesLoading = $state(false);
	let changesOffset = $state(0);
	let loading = $state(false);
	let loadError = $state<string | null>(null);

	const CHANGES_PAGE = 100;

	/** Segment sources already pulled, so a tab loads once. */
	let seenChain = $state(false);
	let seenFiles = $state(false);
	let seenApprovals = $state(false);

	onMount(() => {
		void loadTab(tab);
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		const pending =
			current === 'chain'
				? !seenChain
				: current === 'files'
					? !seenFiles
					: !seenApprovals;
		if (!pending) return;
		loading = true;
		loadError = null;
		try {
			if (current === 'chain') {
				seenChain = true;
				checkpoints = (await listCheckpoints({ limit: 200 })).items;
			} else if (current === 'files') {
				seenFiles = true;
				const page = await getFileChangesPage({ limit: CHANGES_PAGE });
				fileChanges = page.items;
				changesOffset = page.items.length;
				changesHasMore = page.hasMore;
			} else {
				seenApprovals = true;
				approvals = await getApprovalRequests();
			}
		} catch (e) {
			console.error('Failed to load checkpoints segment:', e);
			loadError = e instanceof Error ? e.message : 'Checkpoint request failed';
		} finally {
			loading = false;
		}
	}

	async function loadMoreChanges(): Promise<void> {
		if (changesLoading || !changesHasMore) return;
		changesLoading = true;
		try {
			const page = await getFileChangesPage({
				limit: CHANGES_PAGE,
				offset: changesOffset,
			});
			fileChanges = [...fileChanges, ...page.items];
			changesOffset += page.items.length;
			changesHasMore = page.hasMore;
		} catch (e) {
			console.error('Failed to load more changes:', e);
			toasts.error('More changes unavailable');
		} finally {
			changesLoading = false;
		}
	}

	async function reload(): Promise<void> {
		seenChain = false;
		seenFiles = false;
		seenApprovals = false;
		checkpoints = [];
		fileChanges = [];
		approvals = [];
		changesOffset = 0;
		changesHasMore = false;
		await loadTab(tab);
	}

	const CHANGE_TONE: Record<string, StatusTone> = {
		added: 'success',
		modified: 'info',
		renamed: 'warning',
		deleted: 'danger',
	};

	async function selectChange(change: FileChange): Promise<void> {
		selectedChangeId = change.id;
		stagedDiff = null;
		previewContent = null;
		previewTimeline = null;
		previewTree = null;
		diffNote = null;
		if (!change.actor) {
			diffNote = 'No actor recorded for this change';
			return;
		}
		diffLoading = true;
		try {
			const [diffs, content, timeline, tree] = await Promise.all([
				(async () => {
					if (change.session && change.session !== change.actor) {
						try {
							const paired = await getDiffActors(change.actor, change.session);
							const hit = paired.find((diff) => diff.path === change.path);
							if (hit) return [hit];
						} catch {
							// Fall through to the single-actor staged diff.
						}
					}
					const staged = await getStagedDiffs(change.actor);
					return staged.filter((diff) => diff.path === change.path);
				})(),
				getFileContent(change.actor, change.path).catch(() => null),
				getFileTimeline(change.path).catch(() => null),
				getFileTree(change.actor).catch(() => null),
			]);
			const match = diffs[0] ?? null;
			if (!match) {
				diffNote = 'No staged diff for this path';
			} else {
				stagedDiff = match;
			}
			previewContent = content;
			previewTimeline = timeline;
			previewTree = tree;
		} catch (e) {
			console.error('Failed to load staged diff:', e);
			diffNote = 'Staged diff unavailable';
			toasts.error('Staged diff unavailable');
		} finally {
			diffLoading = false;
		}
	}

	function emptyCopyFor(current: string): {
		title: string;
		description: string;
	} {
		if (current === 'files') {
			return {
				title: 'No file changes',
				description:
					'File deltas appear here once a session writes to the checkpoint workspace.',
			};
		}
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
			? checkpoints.length === 0
			: tab === 'files'
				? fileChanges.length === 0
				: approvals.length === 0,
	);

	const segmentCopy = $derived(emptyCopyFor(tab));

	function isMarkdownPath(path: string): boolean {
		return /\.markdown?$/i.test(path);
	}

	function isJsonText(text: string): boolean {
		const trimmed = text.trim();
		if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return false;
		try {
			JSON.parse(trimmed);
			return true;
		} catch {
			return false;
		}
	}

	function downloadPreview(): void {
		const content = previewContent;
		if (!content) return;
		const name = content.path.split('/').pop() || 'preview';
		const query = `actor=${encodeURIComponent(content.actor)}&path=${encodeURIComponent(content.path)}`;
		void downloadFile(`/api/v1/file-checkpoint/content?${query}`, name).catch(
			() => toasts.error('Download unavailable'),
		);
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
				await rejectApproval(approval.id);
				toasts.warning('Approval rejected');
			}
			seenApprovals = false;
			approvals = [];
			await loadTab(tab);
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error('Approval decision failed');
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
										title={checkpoint.restorable
											? 'Restore this checkpoint'
											: 'This checkpoint is locked and cannot be restored'}
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
											<p class="truncate font-mono text-caption">
												{change.path}
											</p>
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
													toneText(CHANGE_TONE[change.changeType]),
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
						{#snippet footer()}
							<CursorPager
								shown={fileChanges.length}
								hasMore={changesHasMore}
								loading={changesLoading}
								pageSize={CHANGES_PAGE}
								onloadmore={() => void loadMoreChanges()}
							/>
						{/snippet}
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
						{#if previewContent && !previewContent.isBinary && previewContent.content !== null}
							<Card title={previewContent.path}>
								{#snippet actions()}
									<Button variant="ghost" size="sm" onclick={downloadPreview}>
										<Icon name="download" size={13} />
										Download
									</Button>
								{/snippet}
								{#if isMarkdownPath(previewContent.path)}
									<StreamMarkdown content={previewContent.content} done />
								{:else if isJsonText(previewContent.content)}
									<JsonViewer value={previewContent.content} />
								{:else}
									<pre
										class="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro break-words whitespace-pre-wrap text-foreground">{previewContent.content}</pre>
								{/if}
								<p class="mt-1.5 text-micro tabular-nums text-muted-foreground">
									{formatBytes(previewContent.size)}{previewContent.truncated
										? ' · truncated'
										: ''}
								</p>
							</Card>
						{:else if previewContent?.isBinary}
							<Card title={previewContent.path}>
								<p class="text-caption text-muted-foreground">
									Binary content has no text preview
								</p>
							</Card>
						{/if}
						{#if previewTimeline && previewTimeline.entries.length > 0}
							<Card
								title="Version history"
								description="{previewTimeline.total} versions{previewTimeline.truncated
									? ' · truncated'
									: ''}"
							>
								<ul class="space-y-1.5">
									{#each previewTimeline.entries.slice(0, 8) as entry (entry.snapshotId)}
										<li
											class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption"
										>
											<span class="font-mono text-micro text-muted-foreground"
												>{entry.snapshotId.slice(0, 10)}</span
											>
											<span class="min-w-0 flex-1 truncate">{entry.source}</span
											>
											<span
												class="shrink-0 text-micro tabular-nums text-muted-foreground"
												>{formatRelativeTime(
													new Date(entry.timestamp).toISOString(),
												)}</span
											>
										</li>
									{/each}
								</ul>
							</Card>
						{/if}
						{#if previewTree && previewTree.entries.length > 0}
							<Card
								title="Workspace tree"
								description="{previewTree.total} files{previewTree.truncated
									? ' · truncated'
									: ''}"
							>
								<ul class="space-y-1">
									{#each previewTree.entries.slice(0, 10) as entry (entry.path)}
										<li
											class="flex items-center justify-between gap-2 text-caption"
										>
											<span class="min-w-0 flex-1 truncate font-mono"
												>{entry.path}</span
											>
											<span
												class="shrink-0 text-micro tabular-nums text-muted-foreground"
												>{formatBytes(entry.size)}</span
											>
										</li>
									{/each}
								</ul>
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
											onclick={() => void decideApproval(approval, true)}
										>
											<Icon name="check" size={13} />
											Approve
										</Button>
										<Button
											variant="outline"
											size="sm"
											onclick={() => void decideApproval(approval, false)}
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
		</PageState>
	</div>
</div>
