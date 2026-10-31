<script lang="ts">
	import { tick } from 'svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import MonacoEditor from './MonacoEditor.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import type { GutterMark } from '#lib/ide/git-diff-lines';
	import { t } from '#lib/i18n';

	interface Props {
		onRequestClose?: (path: string) => void;
		gutterMarks?: GutterMark[];
		diffOverflow?: boolean;
		onOpenFullDiff?: () => void;
	}

	let { onRequestClose, gutterMarks = [], diffOverflow = false, onOpenFullDiff }: Props = $props();

	let cursor = $state<{ line: number; col: number } | null>(null);
	let dragPath = $state<string | null>(null);

	$effect(() => {
		void ide.activePath;
		cursor = null;
	});

	function fileName(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function isConflictError(message: string | null): boolean {
		if (!message) return false;
		const lower = message.toLowerCase();
		return lower.includes('changed on disk') || lower.includes('conflict');
	}

	async function forceSave(path: string): Promise<void> {
		await ide.saveFile(path, true);
	}

	function onDragStart(path: string, event: DragEvent): void {
		dragPath = path;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			try {
				event.dataTransfer.setData('text/plain', path);
			} catch {
				// Drag payload is best-effort.
			}
		}
	}

	function onTabDrop(path: string, event: DragEvent): void {
		event.preventDefault();
		const from = dragPath ?? (() => {
			try {
				return event.dataTransfer?.getData('text/plain') ?? null;
			} catch {
				return null;
			}
		})();
		dragPath = null;
		if (!from || from === path) return;
		ide.moveTab(from, path);
		void tick();
	}
</script>

<div class="flex h-full min-h-0 flex-col" role="presentation">
	{#if ide.tabs.length === 0}
		<div class="flex flex-1 items-center justify-center px-6 text-center">
			<div class="max-w-sm space-y-2">
				<p class="text-body font-medium">{t('page.ide.noFileOpen')}</p>
				<p class="text-caption text-muted-foreground">{t('page.ide.noFileOpenDesc')}</p>
			</div>
		</div>
	{:else}
		<div
			class="flex items-center gap-1 overflow-x-auto border-b border-border px-2 py-1"
			role="tablist"
		>
			{#each ide.tabs as tab (tab.path)}
				{@const dirty = tab.content !== tab.original}
				<div
					role="tab"
					tabindex={0}
					aria-selected={ide.activePath === tab.path}
					draggable="true"
					ondragstart={(e) => onDragStart(tab.path, e)}
					ondragover={(e) => e.preventDefault()}
					ondrop={(e) => onTabDrop(tab.path, e)}
					class="flex shrink-0 items-center gap-1 rounded-md border px-2 py-1 text-caption {ide.activePath ===
					tab.path
						? 'border-primary bg-primary/10'
						: 'border-border bg-muted/40'}"
				>
					<button
						type="button"
						class="max-w-44 cursor-pointer truncate hover:text-foreground {tab.preview
							? 'italic opacity-70'
							: ''}"
						title={tab.preview
							? `${tab.path} (${t('page.ide.previewHint')})`
							: tab.path}
						onclick={() => {
							ide.activePath = tab.path;
							cursor = null;
						}}
						ondblclick={() => {
							tab.preview = false;
						}}
					>
						{fileName(tab.path)}{dirty ? ' •' : ''}
					</button>
					<IconButton
						icon="x"
						label={t('page.ide.closeTab')}
						compact
						variant="ghost"
						onclick={() => onRequestClose?.(tab.path)}
					/>
				</div>
			{/each}
			{#if ide.dirtyPaths.length > 0}
				<Button size="sm" variant="ghost" onclick={() => void ide.saveAll()}>
					{t('page.ide.saveAll')}
				</Button>
			{/if}
		</div>

		{@const active = ide.active()}
		{#if active}
			<div class="flex min-h-0 flex-1 flex-col">
				{#if active.loading}
					<p class="p-4 text-caption text-muted-foreground">{t('common.loading')}</p>
				{:else if active.error && !active.content}
					<div class="space-y-2 p-4">
						<p class="text-body text-destructive">{active.error}</p>
						<div class="flex flex-wrap gap-2">
							<Button size="sm" variant="outline" onclick={() => void ide.loadFile(active.path)}>
								{t('common.retry')}
							</Button>
							{#if isConflictError(active.error)}
								<Button size="sm" variant="outline" onclick={() => void forceSave(active.path)}>
									{t('page.ide.overwrite')}
								</Button>
							{/if}
						</div>
					</div>
				{:else if active.isBinary}
					<div class="space-y-2 p-4">
						<p class="text-body font-medium">{t('page.ide.binaryTitle')}</p>
						<p class="text-caption text-muted-foreground">{t('page.ide.binaryDesc')}</p>
					</div>
				{:else}
					{#if active.error}
						<div
							class="flex flex-wrap items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 py-1 text-micro text-destructive"
						>
							<span class="min-w-0 flex-1">{active.error}</span>
							{#if isConflictError(active.error)}
								<button
									type="button"
									class="cursor-pointer underline hover:no-underline"
									onclick={() => void ide.loadFile(active.path)}
								>
									{t('page.ide.reloadFile')}
								</button>
								<button
									type="button"
									class="cursor-pointer underline hover:no-underline"
									onclick={() => void forceSave(active.path)}
								>
									{t('page.ide.overwrite')}
								</button>
							{/if}
						</div>
					{/if}
					{#if active.truncated}
						<p
							class="border-b border-warning/30 bg-warning/10 px-3 py-1 text-micro text-warning"
						>
							{t('domain.files.truncatedPath', { path: active.path })}
						</p>
					{/if}
					{#if diffOverflow}
						<div
							class="flex flex-wrap items-center gap-2 border-b border-warning/30 bg-warning/10 px-3 py-1 text-micro text-warning"
						>
							<span class="min-w-0 flex-1">{t('page.ide.diffOverflow')}</span>
							<button
								type="button"
								class="cursor-pointer underline hover:no-underline"
								onclick={() => onOpenFullDiff?.()}
							>
								{t('page.ide.viewDiff')}
							</button>
						</div>
					{/if}
					<div class="min-h-0 flex-1">
						<MonacoEditor
							filePath={active.path}
							value={active.content}
							language={active.language}
							readOnly={active.truncated}
							contentReady={!active.loading}
							{gutterMarks}
							revealLine={ide.pendingReveal?.path === active.path
								? (ide.pendingReveal.line ?? null)
								: null}
							revealCol={ide.pendingReveal?.path === active.path
								? (ide.pendingReveal.col ?? null)
								: null}
							onChange={(next) => ide.updateContent(active.path, next)}
							onSave={() => {
								if (!active.isBinary && !active.saving) void ide.saveFile(active.path);
							}}
							onCursor={(line, col) => {
								if (ide.activePath === active.path) cursor = { line, col };
							}}
						/>
					</div>
					<div
						class="flex items-center justify-between gap-2 border-t border-border px-3 py-1 text-micro text-muted-foreground"
					>
						<span class="min-w-0 flex-1 truncate">{active.path} · {active.language}</span>
						<span class="shrink-0 tabular-nums">
							{cursor && ide.activePath === active.path
								? `${cursor.line}:${cursor.col} · `
								: ''}
							{#if active.saving}
								{t('page.ide.saving')}
							{:else if active.content !== active.original}
								{t('page.ide.unsaved')}
							{:else}
								{t('page.ide.saved')}
							{/if}
						</span>
					</div>
				{/if}
			</div>
		{/if}
	{/if}
</div>
