<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import FileChangesCard from './files/FileChangesCard.svelte';
	import FilePreviewCard from './files/FilePreviewCard.svelte';
	import ReviewMergeCard from './files/ReviewMergeCard.svelte';
	import SessionsCard from './files/SessionsCard.svelte';
	import StorageCard from './files/StorageCard.svelte';
	import SyncCard from './files/SyncCard.svelte';
	import WorkspaceCard from './files/WorkspaceCard.svelte';
	import { downloadFile } from '#lib/api/client';
	import {
		getMainDiffs,
		getDiffActors,
		getFileContent,
		getFileTree,
		getFileTimeline,
		listPartitions,
		listConflicts,
		getPartitionHistory,
		type ConflictFile,
		type FileContent,
		type FileTimeline,
		type FileTree,
		type PartitionCommit,
		type PartitionView,
	} from '#lib/services/checkpoints';
	import type { DiffFile } from '#lib/types/diff';
	import type { FileChange } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as store } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '#lib/stores/fileWorkspace.svelte';
	import { formatRelativeTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	let {
		onresolve,
	}: {
		onresolve: (conflict: ConflictFile) => void;
	} = $props();

	// ── shared read models (partitions feed both the partitions card
	// and the review/merge candidate source; conflicts feed the card and
	// the resolve dialog) ──
	let partitions = $state<PartitionView[]>([]);
	let conflicts = $state<ConflictFile[]>([]);
	const PARTITION_PAGE = 50;
	let partitionShown = $state(PARTITION_PAGE);

	// ── preview orchestration (selection + parallel diff/content/
	// timeline/tree pulls for the selected change) ──
	let selectedChangeId = $state<string | null>(null);
	let mainDiff = $state<DiffFile | null>(null);
	let diffNote = $state<string | null>(null);
	let diffLoading = $state(false);
	let previewContent = $state<FileContent | null>(null);
	let previewTimeline = $state<FileTimeline | null>(null);
	let previewTree = $state<FileTree | null>(null);
	let timelineShown = $state(0);
	let treePrefix = $state('');

	let loading = $state(false);
	let loadError = $state<string | null>(null);

	const TIMELINE_PAGE = 50;

	let selectedPreview = $state<FileChange | null>(null);
	const selectedChange = $derived(selectedPreview);

	// The container follows the files-domain counter for its shared
	// models only. Each card subscribes to the same counter and pulls
	// its own segment; preview pulls stay selection-driven.
	$effect(() => {
		void store.filesVersion;
		void loadShared();
	});

	async function loadShared(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			const [parts, found] = await Promise.all([
				listPartitions().catch(() => []),
				listConflicts().catch(() => []),
			]);
			partitions = parts;
			conflicts = found;
			partitionShown = PARTITION_PAGE;
		} catch (e) {
			console.error('Failed to load file workspace segment:', e);
			loadError = e instanceof Error ? e.message : t('domain.files.requestFailed');
		} finally {
			loading = false;
		}
	}

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
			diffNote = t('domain.files.noActor');
			return;
		}
		if (!workspace.actor) workspace.setActor(change.actor);
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
				diffNote = t('domain.files.noDiff');
			} else {
				mainDiff = match;
			}
			previewContent = content;
			previewTimeline = timeline;
			previewTree = tree;
			timelineShown = Math.min(TIMELINE_PAGE, timeline?.entries.length ?? 0);
		} catch (e) {
			console.error('Failed to load diff against main:', e);
			diffNote = t('domain.files.diffUnavailable');
			toasts.error(t('domain.files.diffUnavailable'));
		} finally {
			diffLoading = false;
		}
	}

	async function reloadTree(): Promise<void> {
		if (!workspace.actor.trim()) {
			toasts.error(t('domain.files.actorRequired'));
			return;
		}
		try {
			previewTree = await getFileTree(workspace.actor.trim(), treePrefix || undefined);
		} catch (e) {
			console.error('Failed to load workspace tree:', e);
			toasts.error(t('domain.files.treeUnavailable'), e instanceof Error ? e.message : undefined);
		}
	}

	function downloadPreview(): void {
		const content = previewContent;
		if (!content) return;
		const name = content.path.split('/').pop() || 'preview';
		const query = `actor=${encodeURIComponent(content.actor)}&path=${encodeURIComponent(content.path)}`;
		void downloadFile(`/api/v1/file-checkpoint/content?${query}`, name).catch(() =>
			toasts.error(t('domain.files.downloadUnavailable')),
		);
	}

	function handleSelect(change: FileChange): void {
		selectedPreview = change;
		void selectChange(change);
	}

	// ── partition commit history ──
	const HISTORY_PAGE = 50;
	let historyOpen = $state(false);
	let historyTarget = $state<PartitionView | null>(null);
	let historyCommits = $state<PartitionCommit[]>([]);
	let historyHasMore = $state(false);
	let historyLoading = $state(false);

	function openHistory(partition: PartitionView): void {
		historyTarget = partition;
		historyCommits = [];
		historyHasMore = false;
		historyOpen = true;
		void loadHistory(0);
	}

	async function loadHistory(offset: number): Promise<void> {
		const target = historyTarget;
		if (!target) return;
		historyLoading = true;
		try {
			const page = await getPartitionHistory(target.partitionId, HISTORY_PAGE, offset);
			historyCommits = offset === 0 ? page.items : [...historyCommits, ...page.items];
			historyHasMore = page.hasMore;
		} catch (e) {
			console.error('Failed to load partition history:', e);
			toasts.error(
				t('domain.files.historyUnavailable'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			historyLoading = false;
		}
	}
</script>

<PageState {loading} error={loadError} onretry={() => void loadShared()}>
	<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
		<FileChangesCard selectedId={selectedChangeId} onselect={handleSelect} />

		<FilePreviewCard
			{selectedChange}
			{mainDiff}
			{diffNote}
			{diffLoading}
			{previewContent}
			{previewTimeline}
			{timelineShown}
			{previewTree}
			{treePrefix}
			ontreeprefix={(value) => (treePrefix = value)}
			onreloadtree={() => void reloadTree()}
			onshowmore={() => {
				timelineShown = Math.min(
					timelineShown + TIMELINE_PAGE,
					previewTimeline?.entries.length ?? timelineShown,
				);
			}}
			ondownload={downloadPreview}
		/>
	</div>
</PageState>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<Card
		title={t('domain.files.partitionsTitle')}
		description={t('domain.files.partitionsDescription', {
			count: partitions.length,
		})}
	>
		{#if partitions.length === 0}
			<p class="text-caption text-muted-foreground">
				{t('domain.files.noPartitions')}
			</p>
		{:else}
			<ul class="space-y-1.5">
				{#each partitions.slice(0, partitionShown) as partition (partition.partitionId)}
					<li class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption">
						<Badge variant="neutral">{partition.kind}</Badge>
						<span class="min-w-0 flex-1 truncate font-mono">{partition.name}</span>
						{#if partition.actor}
							<span class="text-micro text-muted-foreground">{partition.actor}</span>
						{/if}
						<span class="shrink-0 font-mono text-micro text-muted-foreground"
							>{partition.currentSnapshot.slice(0, 10)} · {partition.historyLen}</span
						>
						<Button variant="ghost" size="sm" onclick={() => openHistory(partition)}>
							{t('domain.files.history')}
						</Button>
					</li>
				{/each}
				{#if partitions.length > partitionShown}
					<li>
						<Button variant="ghost" size="sm" onclick={() => (partitionShown += PARTITION_PAGE)}>
							{t('domain.files.showMore', {
								shown: Math.min(PARTITION_PAGE, partitions.length - partitionShown),
								remaining: partitions.length - partitionShown,
							})}
						</Button>
					</li>
				{/if}
			</ul>
		{/if}
	</Card>

	<Card
		title={t('domain.files.conflictsTitle')}
		description={conflicts.length === 0
			? t('domain.files.noConflicts')
			: t('domain.files.conflictsCount', { count: conflicts.length })}
	>
		{#if conflicts.length === 0}
			<p class="text-caption text-muted-foreground">
				{t('domain.files.conflictsHint')}
			</p>
		{:else}
			<ul class="space-y-2">
				{#each conflicts as conflict (`${conflict.partition}:${conflict.path}`)}
					<li class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption">
						<span class="min-w-0 flex-1 truncate font-mono">{conflict.path}</span>
						<span class="text-micro text-muted-foreground">
							{t('domain.files.regions', {
								partition: conflict.partition,
								count: conflict.conflictCount,
							})}
						</span>
						<Button
							variant="outline"
							size="sm"
							disabled={store.busy || workspace.busy}
							onclick={() => onresolve(conflict)}
						>
							{t('domain.checkpoint.resolve')}
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>
</div>

<ReviewMergeCard {partitions} />

<Dialog
	bind:open={historyOpen}
	title={t('domain.files.historyTitle')}
	description={historyTarget
		? t('domain.files.historyDescription', { name: historyTarget.name })
		: t('domain.files.historyDescriptionFallback')}
	onclose={() => (historyTarget = null)}
>
	{#if historyTarget}
		<p class="mb-2 font-mono text-micro text-muted-foreground">
			{historyTarget.partitionId}
		</p>
	{/if}
	{#if historyCommits.length === 0}
		<p class="text-caption text-muted-foreground">
			{historyLoading ? t('common.loading') : t('domain.files.noCommits')}
		</p>
	{:else}
		<ul class="max-h-96 space-y-2 overflow-y-auto">
			{#each historyCommits as commit (commit.snapshotId)}
				<li class="border-b border-border pb-2 last:border-b-0">
					<div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption">
						<span class="font-mono">{commit.snapshotId.slice(0, 10)}</span>
						<span class="min-w-0 flex-1 truncate">
							{commit.message ?? t('domain.files.noMessage')}
						</span>
						{#if commit.isMerge}
							<Badge variant="neutral">{t('domain.files.merge')}</Badge>
						{/if}
						{#if commit.hasConflicts}
							<Badge variant="warning">{t('domain.files.conflicts')}</Badge>
						{/if}
						<span class="shrink-0 text-micro text-muted-foreground">
							{formatRelativeTime(commit.timestamp)}
						</span>
					</div>
					<div class="text-micro text-muted-foreground">
						{t('domain.files.commitMeta', {
							actor: commit.actor ?? t('domain.files.unknownActor'),
							session: commit.session ?? '',
							tool: commit.tool ?? '',
							parents: commit.parents.length,
						})}
					</div>
				</li>
			{/each}
		</ul>
		{#if historyHasMore}
			<Button
				variant="ghost"
				size="sm"
				class="mt-2"
				disabled={historyLoading}
				onclick={() => void loadHistory(historyCommits.length)}
			>
				{t('domain.files.loadMore')}
			</Button>
		{/if}
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (historyOpen = false)}>
			{t('common.close')}
		</Button>
	{/snippet}
</Dialog>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<SessionsCard />
	<WorkspaceCard />
</div>

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<StorageCard />
	<SyncCard />
</div>
