<script lang="ts">
	import { onMount } from 'svelte';
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import ResizeHandle from '#lib/components/layout/ResizeHandle.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		repo: string;
	}

	let { repo }: Props = $props();

	let panel: PtyPanel | undefined = $state();
	let maximizedHeight = $state(480);

	function measureMaximized(): number {
		if (typeof window === 'undefined') return 480;
		const target = Math.round(window.innerHeight * 0.55);
		return Math.min(640, Math.max(240, target));
	}

	function refreshMaximized(): void {
		maximizedHeight = measureMaximized();
	}

	onMount(() => {
		refreshMaximized();
		if (typeof window === 'undefined') return;
		window.addEventListener('resize', refreshMaximized);
		return () => window.removeEventListener('resize', refreshMaximized);
	});

	let cwd = $derived(repo ? repo : undefined);
	let height = $derived(ide.terminalMaximized ? maximizedHeight : ide.terminalHeight);

	function onResizeHandle(delta: number): void {
		ide.setTerminalHeight(ide.terminalHeight + delta);
	}

	$effect(() => {
		if (ide.terminalOpen) panel?.focusActive();
	});
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
			onReset={() => ide.resetTerminalHeight()}
		/>
		<IconButton
			icon={ide.terminalMaximized ? 'chevron-down' : 'chevron-up'}
			label={ide.terminalMaximized ? t('page.ide.restoreTerminal') : t('page.ide.toggleMaximize')}
			compact
			variant="ghost"
			onclick={() => ide.setTerminalMaximized(!ide.terminalMaximized)}
		/>
		<IconButton
			icon="x"
			label={t('page.ide.hideTerminal')}
			compact
			variant="ghost"
			onclick={() => ide.setTerminalOpen(false)}
		/>
	</div>
	<div class="min-h-0 shrink-0 overflow-hidden px-3 pb-3" style="height: {height}px">
		{#if repo}
			<PtyPanel
				bind:this={panel}
				{repo}
				{cwd}
				fill
				embedded
				open={ide.terminalOpen}
				onClose={() => {
					ide.setTerminalOpen(false);
				}}
			/>
		{:else}
			<div class="flex h-full items-center px-1 text-caption text-muted-foreground">
				{t('common.loading')}
			</div>
		{/if}
	</div>
</div>
