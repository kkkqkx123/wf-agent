<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		repo: string;
	}

	let { repo }: Props = $props();

	let cwd = $derived(repo ? repo : undefined);

	let dragging = $state(false);

	function onPointerDown(event: PointerEvent): void {
		dragging = true;
		(event.target as HTMLElement).setPointerCapture(event.pointerId);
		const startY = event.clientY;
		const startHeight = ide.terminalHeight;
		const move = (e: PointerEvent): void => {
			if (!dragging) return;
			ide.setTerminalHeight(startHeight + (startY - e.clientY));
		};
		const up = (): void => {
			dragging = false;
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}
</script>

<div class="flex min-h-0 flex-col border-t border-border bg-card">
	<div
		class="flex cursor-ns-resize items-center justify-center py-1 text-muted-foreground hover:text-foreground"
		onpointerdown={onPointerDown}
		role="separator"
		aria-orientation="horizontal"
		aria-label="Resize terminal"
	>
		<div class="h-1 w-16 rounded-full bg-border"></div>
	</div>
	<div class="min-h-0 flex-1 overflow-hidden px-3 pb-3" style="height: {ide.terminalHeight}px">
		<PtyPanel
			repo={repo || 'ide'}
			cwd={cwd}
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
