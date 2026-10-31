<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import ResizeHandle from '#lib/components/layout/ResizeHandle.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		repo: string;
	}

	let { repo }: Props = $props();

	let cwd = $derived(repo ? repo : undefined);
	let height = $derived(ide.terminalMaximized ? 560 : ide.terminalHeight);

	function onResizeHandle(delta: number): void {
		ide.setTerminalHeight(ide.terminalHeight + delta);
	}
</script>

<div class="flex min-h-0 flex-col bg-card">
	<div class="flex items-center gap-1 px-3 pt-1">
		<ResizeHandle
			label={t('page.ide.resizeTerminal')}
			step={24}
			disabled={ide.terminalMaximized}
			class="flex-1 py-1 text-muted-foreground hover:text-foreground"
			pillClass="h-1 w-16 bg-border"
			onResize={onResizeHandle}
		/>
		<IconButton
			icon={ide.terminalMaximized ? 'chevron-down' : 'chevron-up'}
			label={ide.terminalMaximized ? t('page.ide.restoreTerminal') : t('page.ide.toggleMaximize')}
			compact
			variant="ghost"
			onclick={() => ide.setTerminalMaximized(!ide.terminalMaximized)}
		/>
	</div>
	<div class="min-h-0 flex-1 overflow-hidden px-3 pb-3" style="height: {height}px">
		<PtyPanel
			repo={repo || 'ide'}
			{cwd}
			open={ide.terminalOpen}
			onClose={() => {
				ide.terminalOpen = false;
			}}
		/>
		{#if !ide.terminalOpen}
			<button
				type="button"
				class="cursor-pointer rounded-md border border-border px-2 py-1 text-caption hover:bg-accent"
				onclick={() => {
					ide.terminalOpen = true;
				}}
			>
				{t('page.ide.showTerminal')}
			</button>
		{/if}
	</div>
</div>
