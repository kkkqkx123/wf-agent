<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import FileChangesCard from './files/FileChangesCard.svelte';
	import FilePreviewCard from './files/FilePreviewCard.svelte';
	import ReviewMergeCard from './files/ReviewMergeCard.svelte';
	import SessionsCard from './files/SessionsCard.svelte';
	import WorkspaceCard from './files/WorkspaceCard.svelte';
	import { downloadFile } from '$lib/api/client';
	import {
		getMainDiffs,
		getDiffActors,
		getFileContent,
		getFileTree,
		getFileTimeline,
		listPartitions,
		listConflicts,
		type ConflictFile,
		type FileContent,
		type FileTimeline,
		type FileTree,
		type PartitionView,
	} from '$lib/services/checkpoints';
	import type { DiffFile } from '$lib/types/diff';
	import type { FileChange } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';

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
		} catch (e) {
			console.error('Failed to load file workspace segment:', e);
			loadError =
				e instanceof Error ? e.message : 'File workspace request failed';
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
			diffNote = 'No actor recorded for this change';
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
		if (!workspace.actor.trim()) {
			toasts.error('Enter an actor to browse the workspace tree');
			return;
		}
		try {
			previewTree = await getFileTree(
				workspace.actor.trim(),
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

	function downloadPreview(): void {
		const content = previewContent;
		if (!content) return;
		const name = content.path.split('/').pop() || 'preview';
		const query = `actor=${encodeURIComponent(content.actor)}&path=${encodeURIComponent(content.path)}`;
		void downloadFile(`/api/v1/file-checkpoint/content?${query}`, name).catch(
			() => toasts.error('Download unavailable'),
		);
	}

	function handleSelect(change: FileChange): void {
		selectedPreview = change;
		void selectChange(change);
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
							disabled={store.busy || workspace.busy}
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

<ReviewMergeCard {partitions} />

<div class="mt-3 grid gap-3 xl:grid-cols-2">
	<SessionsCard />
	<WorkspaceCard />
</div>
