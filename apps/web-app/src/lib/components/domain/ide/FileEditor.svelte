<script lang="ts">
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import MonacoEditor from './MonacoEditor.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		onRequestClose?: (path: string) => void;
	}

	let { onRequestClose }: Props = $props();

	function fileName(path: string): string {
		return path.split('/').pop() ?? path;
	}

	async function handleKeydown(event: KeyboardEvent): Promise<void> {
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
			event.preventDefault();
			const active = ide.active();
			if (active && !active.isBinary) await ide.saveFile(active.path);
		}
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'w') {
			event.preventDefault();
			const active = ide.active();
			if (active) onRequestClose?.(active.path);
		}
	}
</script>

<div
	class="flex h-full min-h-0 flex-col"
	onkeydown={(e) => void handleKeydown(e)}
	role="presentation"
>
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
					aria-selected={ide.activePath === tab.path}
					class="flex shrink-0 items-center gap-1 rounded-md border px-2 py-1 text-caption {ide.activePath ===
					tab.path
						? 'border-primary bg-primary/10'
						: 'border-border bg-muted/40'}"
				>
					<button
						type="button"
						class="max-w-44 cursor-pointer truncate hover:text-foreground"
						title={tab.path}
						onclick={() => {
							ide.activePath = tab.path;
						}}
						ondblclick={() => {
							tab.preview = false;
						}}
					>
						{fileName(tab.path)}{tab.preview ? ` (${t('page.ide.preview')})` : ''}{dirty
							? ' •'
							: ''}
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

		{#key ide.activePath}
			{@const active = ide.active()}
			{#if active}
				<div class="flex min-h-0 flex-1 flex-col">
					{#if active.loading}
						<p class="p-4 text-caption text-muted-foreground">{t('common.loading')}</p>
					{:else if active.error && !active.content}
						<div class="space-y-2 p-4">
							<p class="text-body text-destructive">{active.error}</p>
							<Button size="sm" variant="outline" onclick={() => void ide.loadFile(active.path)}>
								{t('common.retry')}
							</Button>
						</div>
					{:else if active.isBinary}
						<div class="space-y-2 p-4">
							<p class="text-body font-medium">{t('page.ide.binaryTitle')}</p>
							<p class="text-caption text-muted-foreground">{t('page.ide.binaryDesc')}</p>
						</div>
					{:else}
						{#if active.error}
							<p
								class="border-b border-destructive/30 bg-destructive/10 px-3 py-1 text-micro text-destructive"
							>
								{active.error}
							</p>
						{/if}
						<div class="min-h-0 flex-1">
							<MonacoEditor
								filePath={active.path}
								value={active.content}
								language={active.language}
								readOnly={false}
								onChange={(next) => ide.updateContent(active.path, next)}
							/>
						</div>
						<div
							class="flex items-center justify-between border-t border-border px-3 py-1 text-micro text-muted-foreground"
						>
							<span>{active.path} · {active.language}</span>
							<span>
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
		{/key}
	{/if}
</div>
