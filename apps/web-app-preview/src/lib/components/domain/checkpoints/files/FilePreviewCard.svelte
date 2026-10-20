<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import DiffView from '@wf-agent/ui/components/DiffView.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import StreamMarkdown from '$lib/components/chat/LazyStreamMarkdown.svelte';
	import type {
		FileContent,
		FileTimeline,
		FileTree,
	} from '$lib/services/checkpoints';
	import type { DiffFile } from '$lib/types/diff';
	import type { FileChange } from '$lib/types/models';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';
	import { formatBytes, formatRelativeTime } from '$lib/utils/format';

	let {
		selectedChange,
		mainDiff,
		diffNote,
		diffLoading,
		previewContent,
		previewTimeline,
		timelineShown,
		previewTree,
		treePrefix,
		ontreeprefix,
		onreloadtree,
		onshowmore,
		ondownload,
	}: {
		selectedChange: FileChange | null;
		mainDiff: DiffFile | null;
		diffNote: string | null;
		diffLoading: boolean;
		previewContent: FileContent | null;
		previewTimeline: FileTimeline | null;
		timelineShown: number;
		previewTree: FileTree | null;
		treePrefix: string;
		ontreeprefix: (value: string) => void;
		onreloadtree: () => void;
		onshowmore: () => void;
		ondownload: () => void;
	} = $props();

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
</script>

<div class="space-y-3">
	{#if diffLoading}
		<Card title="Selected delta">
			<Skeleton shape="block" height="6rem" />
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
				<Button variant="ghost" size="sm" onclick={ondownload}>
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
						<span class="shrink-0 text-micro tabular-nums text-muted-foreground"
							>{formatRelativeTime(
								new Date(entry.timestamp).toISOString(),
							)}</span
						>
					</li>
				{/each}
			</ul>
			{#snippet footer()}
				{#if timelineShown < timelineEntries.length}
					<Button variant="ghost" size="sm" onclick={onshowmore}>
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
					value={treePrefix}
					oninput={(event: Event) =>
						ontreeprefix((event.target as HTMLInputElement).value)}
					placeholder="src/lib (empty lists all)"
				/>
			</div>
			<Button
				variant="outline"
				size="sm"
				disabled={!workspace.actor.trim()}
				onclick={onreloadtree}
			>
				Apply filter
			</Button>
		</div>
		{#if previewTree && previewTree.entries.length > 0}
			<ul class="space-y-1">
				{#each previewTree.entries as entry (entry.path)}
					<li class="flex items-center justify-between gap-2 text-caption">
						<span class="min-w-0 flex-1 truncate font-mono">{entry.path}</span>
						<span class="shrink-0 text-micro tabular-nums text-muted-foreground"
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
