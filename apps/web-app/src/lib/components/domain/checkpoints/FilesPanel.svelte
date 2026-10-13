<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import DiffView from '@wf-agent/ui/components/DiffView.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import CursorPager from '@wf-agent/ui/components/CursorPager.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import type { StatusTone } from '@wf-agent/ui/status';
	import { downloadFile } from '$lib/api/client';
	import {
		getFileChangesPage,
		getMainDiffs,
		getDiffActors,
		getFileContent,
		getFileTree,
		getFileTimeline,
		listPartitions,
		listConflicts,
		listEditSessions,
		beginEditSession,
		rollbackEditSession,
		undoEdit,
		redoEdit,
		getActorWorkspace,
		getIndexStatus,
		rebuildIndex,
		submitForReview,
		mergeToFeature,
		mergeToMain,
		rollbackFeature,
		type ConflictFile,
		type EditSession,
		type FileContent,
		type FileDiff,
		type FileTimeline,
		type FileTree,
		type IndexStatus,
		type PartitionView,
		type WorkspaceFile,
	} from '$lib/services/checkpoints';
	import type { FileChange } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { formatBytes, formatRelativeTime } from '$lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';

	let {
		onresolve,
	}: {
		onresolve: (conflict: ConflictFile) => void;
	} = $props();

	// ── file segment (owned here; the page only keeps the event stream
	// and dialogs) ──
	let fileChanges = $state<FileChange[]>([]);
	let partitions = $state<PartitionView[]>([]);
	let conflicts = $state<ConflictFile[]>([]);
	let sessions = $state<EditSession[]>([]);
	let workspaceFiles = $state<WorkspaceFile[]>([]);
	let indexStatus = $state<IndexStatus | null>(null);

	let selectedChangeId = $state<string | null>(null);
	let mainDiff = $state<FileDiff | null>(null);
	let diffNote = $state<string | null>(null);
	let diffLoading = $state(false);
	let previewContent = $state<FileContent | null>(null);
	let previewTimeline = $state<FileTimeline | null>(null);
	let previewTree = $state<FileTree | null>(null);
	let timelineShown = $state(0);
	let treePrefix = $state('');
	let reviewFeature = $state('');
	let reviewFeaturesText = $state('');
	let rollbackHead = $state('');

	let changesHasMore = $state(false);
	let changesLoading = $state(false);
	let changesOffset = $state(0);
	let loading = $state(false);
	let loadError = $state<string | null>(null);

	const CHANGES_PAGE = 100;
	const TIMELINE_PAGE = 50;

	// The panel follows the store version: SSE events bump it from the
	// page-side subscription and local mutations bump it directly, so this
	// effect re-pulls the segment on every change (and on mount).
	$effect(() => {
		void store.version;
		void loadSegment();
	});

	async function loadSegment(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			const [changes, parts, found, groups, index] = await Promise.all([
				getFileChangesPage({ limit: CHANGES_PAGE }),
				listPartitions().catch(() => []),
				listConflicts().catch(() => []),
				listEditSessions().catch(() => []),
				getIndexStatus().catch(() => null),
			]);
			fileChanges = changes.items;
			changesOffset = changes.items.length;
			changesHasMore = changes.hasMore;
			partitions = parts;
			conflicts = found;
			sessions = groups;
			indexStatus = index;
		} catch (e) {
			console.error('Failed to load file workspace segment:', e);
			loadError =
				e instanceof Error ? e.message : 'File workspace request failed';
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

	const CHANGE_TONE: Record<string, StatusTone> = {
		added: 'success',
		modified: 'info',
		renamed: 'warning',
		deleted: 'danger',
	};

	async function selectChange(change: FileChange): Promise<void> {
		selectedChangeId = change.id;
		store.select('files', change.id);
		mainDiff = null;
		previewContent = null;
		previewTimeline = null;
		previewTree = null;
		timelineShown = 0;
		diffNote = null;
		if (!change.actor) {
			diffNote = 'No actor recorded for this change';
			return;
		}
		if (!store.actor) store.actor = change.actor;
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
							// Fall through to the single-actor diff against main.
						}
					}
					const diffs = await getMainDiffs(change.actor);
					return diffs.filter((diff) => diff.path === change.path);
				})(),
				getFileContent(change.actor, change.path).catch(() => null),
				getFileTimeline(change.path).catch(() => null),
				getFileTree(change.actor, treePrefix || undefined).catch(() => null),
			]);
			const match = diffs[0] ?? null;
			if (!match) {
				diffNote = 'No diff against main for this path';
			} else {
				mainDiff = match;
			}
			previewContent = content;
			previewTimeline = timeline;
			previewTree = tree;
			timelineShown = Math.min(TIMELINE_PAGE, timeline?.entries.length ?? 0);
		} catch (e) {
			console.error('Failed to load diff against main:', e);
			diffNote = 'Diff against main unavailable';
			toasts.error('Diff against main unavailable');
		} finally {
			diffLoading = false;
		}
	}

	async function reloadTree(): Promise<void> {
		if (!store.actor.trim()) {
			toasts.error('Enter an actor to browse the workspace tree');
			return;
		}
		try {
			previewTree = await getFileTree(
				store.actor.trim(),
				treePrefix || undefined,
			);
		} catch (e) {
			console.error('Failed to load workspace tree:', e);
			toasts.error(
				'Workspace tree unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function loadWorkspace(): Promise<void> {
		if (!store.actor.trim()) {
			toasts.error('Enter an actor to inspect the workspace');
			return;
		}
		try {
			workspaceFiles = await getActorWorkspace(store.actor.trim());
		} catch (e) {
			console.error('Failed to load actor workspace:', e);
			toasts.error(
				'Workspace unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runUndo(): Promise<void> {
		const actor = store.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor for undo');
			return;
		}
		store.beginAction('undo');
		try {
			const snapshot = await undoEdit(actor);
			toasts.success('Undo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			store.bump();
		} catch (e) {
			console.error('Undo failed:', e);
			store.endAction('Undo failed');
			toasts.error('Undo failed', e instanceof Error ? e.message : undefined);
			return;
		}
		store.endAction();
	}

	async function runRedo(): Promise<void> {
		const actor = store.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor for redo');
			return;
		}
		store.beginAction('redo');
		try {
			const snapshot = await redoEdit(actor);
			toasts.success('Redo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			store.bump();
		} catch (e) {
			console.error('Redo failed:', e);
			store.endAction('Redo failed');
			toasts.error('Redo failed', e instanceof Error ? e.message : undefined);
			return;
		}
		store.endAction();
	}

	async function runRebuildIndex(): Promise<void> {
		store.beginAction('rebuild');
		try {
			const { indexedCommits } = await rebuildIndex();
			indexStatus = await getIndexStatus().catch(() => null);
			toasts.success(
				'Source index rebuilt',
				`${indexedCommits} commits indexed`,
			);
		} catch (e) {
			console.error('Index rebuild failed:', e);
			store.endAction('Rebuild failed');
			toasts.error(
				'Index rebuild failed',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		store.endAction();
	}

	async function runBeginSession(): Promise<void> {
		store.beginAction('begin-session');
		try {
			const id = await beginEditSession();
			toasts.success('Edit session started', id);
			sessions = await listEditSessions().catch(() => sessions);
		} catch (e) {
			console.error('Begin session failed:', e);
			toasts.error(
				'Begin session failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			store.endAction();
		}
	}

	async function runRollbackSession(session: EditSession): Promise<void> {
		const actor = store.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor to roll back the session');
			return;
		}
		store.beginAction(`rollback-${session.id}`);
		try {
			const snapshot = await rollbackEditSession(session.id, actor);
			toasts.success(
				'Session rolled back',
				`Snapshot ${snapshot.slice(0, 10)}`,
			);
			sessions = await listEditSessions().catch(() => sessions);
			store.bump();
		} catch (e) {
			console.error('Session rollback failed:', e);
			toasts.error(
				'Session rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			store.endAction();
		}
	}

	async function runSubmitReview(): Promise<void> {
		const actor = store.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor to submit for review');
			return;
		}
		store.beginAction('submit');
		try {
			const { reviewCommit } = await submitForReview(actor);
			toasts.success('Submitted for review', reviewCommit.slice(0, 10));
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Submit for review failed:', e);
			toasts.error('Submit failed', e instanceof Error ? e.message : undefined);
		} finally {
			store.endAction();
		}
	}

	async function runMergeToFeature(): Promise<void> {
		const actor = store.actor.trim();
		const feature = reviewFeature.trim();
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (!feature) {
			toasts.error('Enter a feature name for refs/wf/feat/<name>');
			return;
		}
		store.beginAction('merge-feature');
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
			conflicts = await listConflicts().catch(() => conflicts);
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Merge to feature failed:', e);
			toasts.error('Merge failed', e instanceof Error ? e.message : undefined);
		} finally {
			store.endAction();
		}
	}

	async function runMergeToMain(): Promise<void> {
		const actor = store.actor.trim();
		const features = reviewFeaturesText
			.split(/[,\s]+/)
			.map((part) => part.trim())
			.filter((part) => part.length > 0);
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (features.length === 0) {
			toasts.error('Enter at least one feature name');
			return;
		}
		store.beginAction('merge-main');
		try {
			const outcome = await mergeToMain(actor, features);
			if (outcome.conflictFiles.length > 0) {
				toasts.warning(
					'Merge to main has conflicts',
					outcome.conflictFiles.slice(0, 3).join(', '),
				);
			} else {
				toasts.success('Merged to main', outcome.checkpointId.slice(0, 10));
			}
			conflicts = await listConflicts().catch(() => conflicts);
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Merge to main failed:', e);
			toasts.error(
				'Merge to main failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			store.endAction();
		}
	}

	async function runRollbackMergedFeature(): Promise<void> {
		const actor = store.actor.trim();
		const head = rollbackHead.trim();
		if (!actor) {
			toasts.error('Enter an actor for rollback');
			return;
		}
		if (!head) {
			toasts.error('Enter the merged feature head commit to revert');
			return;
		}
		store.beginAction('rollback-feature');
		try {
			const { revertCommit } = await rollbackFeature(actor, head);
			toasts.success('Feature rolled back', revertCommit.slice(0, 10));
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Feature rollback failed:', e);
			toasts.error(
				'Rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			store.endAction();
		}
	}

	const selectedChange = $derived(
		fileChanges.find((change) => change.id === selectedChangeId) ?? null,
	);

	const timelineEntries = $derived(previewTimeline?.entries ?? []);
	const timelineVisible = $derived(timelineEntries.slice(0, timelineShown));

	function isMarkdownPath(path: string): boolean {
		return /\.(md|markdown)$/i.test(path);
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
</script>

<PageState {loading} error={loadError} onretry={() => void loadSegment()}>
	<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
		<Card title="Changes" description="Recent file deltas grouped by session">
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
								<span class="text-micro tabular-nums text-success">
									+{change.additions}
								</span>
								<span class="text-micro tabular-nums text-destructive">
									-{change.deletions}
								</span>
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
						Loading diff against main…
					</p>
				</Card>
			{:else if mainDiff && !mainDiff.binary}
				<DiffView
					lines={mainDiff.lines}
					title={mainDiff.truncated
						? `${mainDiff.path} · truncated`
						: mainDiff.path}
				/>
			{:else if mainDiff?.binary}
				<Card title={mainDiff.path}>
					<p class="text-caption text-muted-foreground">
						Binary content has no text diff
					</p>
				</Card>
			{:else}
				<Card title="Selected delta">
					<p class="text-caption text-muted-foreground">
						{diffNote ?? 'Select a change to preview its diff against main'}
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
			{#if previewTimeline && timelineEntries.length > 0}
				<Card
					title="Version history"
					description="{previewTimeline.total} versions{previewTimeline.truncated
						? ' · truncated at 5000'
						: ''}{selectedChange?.path ? ` · ${selectedChange.path}` : ''}"
				>
					<ul class="space-y-1.5">
						{#each timelineVisible as entry (entry.snapshotId)}
							<li
								class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption"
							>
								<span class="font-mono text-micro text-muted-foreground"
									>{entry.snapshotId.slice(0, 10)}</span
								>
								<span class="min-w-0 flex-1 truncate">{entry.source}</span>
								{#if entry.movedFrom}
									<span class="text-micro text-muted-foreground">
										renamed from {entry.movedFrom}
									</span>
								{/if}
								<span
									class="shrink-0 text-micro tabular-nums text-muted-foreground"
									>{formatRelativeTime(
										new Date(entry.timestamp).toISOString(),
									)}</span
								>
							</li>
						{/each}
					</ul>
					{#snippet footer()}
						{#if timelineShown < timelineEntries.length}
							<Button
								variant="ghost"
								size="sm"
								onclick={() => {
									timelineShown = Math.min(
										timelineShown + TIMELINE_PAGE,
										timelineEntries.length,
									);
								}}
							>
								Show more ({timelineEntries.length - timelineShown} remaining)
							</Button>
						{/if}
					{/snippet}
				</Card>
			{/if}
			<Card
				title="Workspace tree"
				description={previewTree
					? `${previewTree.total} files${previewTree.truncated ? ' · truncated at 2000' : ''}`
					: 'Browse an actor workspace by path prefix'}
			>
				<div class="mb-2 flex flex-wrap items-end gap-2">
					<div class="min-w-40 flex-1">
						<Input
							label="Path prefix"
							size="sm"
							bind:value={treePrefix}
							placeholder="src/lib (empty lists all)"
						/>
					</div>
					<Button
						variant="outline"
						size="sm"
						disabled={!store.actor.trim()}
						onclick={() => void reloadTree()}
					>
						Apply filter
					</Button>
				</div>
				{#if previewTree && previewTree.entries.length > 0}
					<ul class="space-y-1">
						{#each previewTree.entries as entry (entry.path)}
							<li class="flex items-center justify-between gap-2 text-caption">
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
				{:else}
					<p class="text-caption text-muted-foreground">
						No tree loaded yet — select a change or apply a filter.
					</p>
				{/if}
			</Card>
		</div>
	</div>
</PageState>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<Card
		title="Partitions"
		description="{partitions.length} ref lines in the checkpoint store"
	>
		{#if partitions.length === 0}
			<p class="text-caption text-muted-foreground">No partitions reported.</p>
		{:else}
			<ul class="space-y-1.5">
				{#each partitions as partition (partition.partitionId)}
					<li
						class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption"
					>
						<Badge variant="neutral">{partition.kind}</Badge>
						<span class="min-w-0 flex-1 truncate font-mono"
							>{partition.name}</span
						>
						{#if partition.actor}
							<span class="text-micro text-muted-foreground"
								>{partition.actor}</span
							>
						{/if}
						<span class="shrink-0 font-mono text-micro text-muted-foreground"
							>{partition.currentSnapshot.slice(0, 10)} · {partition.historyLen}</span
						>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>

	<Card
		title="Conflicts"
		description={conflicts.length === 0
			? 'No unresolved merge conflicts'
			: `${conflicts.length} file(s) with unresolved merges`}
	>
		{#if conflicts.length === 0}
			<p class="text-caption text-muted-foreground">
				Conflicts are enumerated from unresolved merge commits.
			</p>
		{:else}
			<ul class="space-y-2">
				{#each conflicts as conflict (`${conflict.partition}:${conflict.path}`)}
					<li class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption">
						<span class="min-w-0 flex-1 truncate font-mono"
							>{conflict.path}</span
						>
						<span class="text-micro text-muted-foreground">
							{conflict.partition} · {conflict.conflictCount} regions
						</span>
						<Button
							variant="outline"
							size="sm"
							disabled={store.busy}
							onclick={() => onresolve(conflict)}
						>
							Resolve
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>
</div>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<Card
		title="Review & merge"
		description="Submit edit lines, merge features, roll back merged features"
	>
		<div class="grid gap-2 sm:grid-cols-2">
			<Input
				label="Actor"
				size="sm"
				bind:value={store.actor}
				placeholder="Actor partition"
			/>
			<Input
				label="Feature"
				size="sm"
				bind:value={reviewFeature}
				placeholder="Feature name for refs/wf/feat/<name>"
			/>
		</div>
		<div class="mt-2">
			<Input
				label="Features for main (comma or space separated)"
				size="sm"
				bind:value={reviewFeaturesText}
				placeholder="demo, next"
			/>
		</div>
		<div class="mt-2 flex flex-wrap gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runSubmitReview()}
			>
				Submit for review
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runMergeToFeature()}
			>
				Merge to feature
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runMergeToMain()}
			>
				Merge to main
			</Button>
		</div>
	</Card>

	<Card
		title="Feature rollback"
		description="Append a revert commit; other merged features stay untouched"
	>
		<Input
			label="Merged feature head commit"
			size="sm"
			bind:value={rollbackHead}
			placeholder="Merge commit id to revert"
		/>
		<div class="mt-2">
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy || !rollbackHead.trim()}
				onclick={() => void runRollbackMergedFeature()}
			>
				Roll back feature
			</Button>
		</div>
	</Card>
</div>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<Card
		title="Edit sessions"
		description="Staged multi-file operations; rollback appends a revert commit"
	>
		{#snippet actions()}
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runBeginSession()}
			>
				<Icon name="plus" size={13} />
				New session
			</Button>
		{/snippet}
		<div class="mb-2">
			<Input
				label="Rollback actor"
				size="sm"
				bind:value={store.actor}
				placeholder="Actor owning the session partition"
			/>
		</div>
		{#if sessions.length === 0}
			<p class="text-caption text-muted-foreground">No staged edit sessions.</p>
		{:else}
			<ul class="space-y-2">
				{#each sessions as session (session.id)}
					<li class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption">
						<span class="min-w-0 flex-1 truncate font-mono">{session.id}</span>
						{#if session.label}
							<span class="text-micro text-muted-foreground"
								>{session.label}</span
							>
						{/if}
						<Button
							variant="outline"
							size="sm"
							disabled={store.busy || !store.actor.trim()}
							title="Roll back this session on the actor partition"
							onclick={() => void runRollbackSession(session)}
						>
							Rollback
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>

	<Card
		title="Workspace & index"
		description="Actor undo/redo, workspace inspection and source-index health"
	>
		<div class="mb-2">
			<Input
				label="Actor"
				size="sm"
				bind:value={store.actor}
				placeholder="Actor partition"
			/>
		</div>
		<div class="flex flex-wrap gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runUndo()}
			>
				<Icon name="arrow-left" size={13} />
				Undo
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runRedo()}
			>
				Redo
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void loadWorkspace()}
			>
				Inspect workspace
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={store.busy}
				onclick={() => void runRebuildIndex()}
			>
				<Icon name="database" size={13} />
				Rebuild index
			</Button>
		</div>
		{#if store.actionError}
			<p class="mt-2 text-caption text-destructive">{store.actionError}</p>
		{/if}
		{#if indexStatus}
			<p class="mt-2 text-caption tabular-nums text-muted-foreground">
				Index: {indexStatus.indexedCommits}/{indexStatus.totalCommits} commits{indexStatus.needsRebuild
					? ' · rebuild recommended'
					: ' · healthy'}
			</p>
		{/if}
		{#if workspaceFiles.length > 0}
			<ul class="mt-2 space-y-1">
				{#each workspaceFiles.slice(0, 20) as file (file.path)}
					<li class="flex items-center justify-between gap-2 text-caption">
						<span class="min-w-0 flex-1 truncate font-mono">{file.path}</span>
						<span class="shrink-0 font-mono text-micro text-muted-foreground"
							>{file.hash.slice(0, 10)}</span
						>
					</li>
				{/each}
			</ul>
			{#if workspaceFiles.length > 20}
				<p class="mt-1 text-micro text-muted-foreground">
					Showing 20 of {workspaceFiles.length} files.
				</p>
			{/if}
		{/if}
	</Card>
</div>
