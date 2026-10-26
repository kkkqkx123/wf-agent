<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		repo: string;
	}

	let { repo }: Props = $props();

	let cwd = $derived(repo ? repo : undefined);
	let height = $derived(ide.terminalMaximized ? 560 : ide.terminalHeight);

	let dragging = $state(false);

	function onPointerDown(event: PointerEvent): void {
		if (ide.terminalMaximized) return;
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

	function onSeparatorKey(event: KeyboardEvent): void {
		if (ide.terminalMaximized) return;
		if (event.key === 'ArrowUp') {
			event.preventDefault();
			ide.setTerminalHeight(ide.terminalHeight + 24);
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			ide.setTerminalHeight(ide.terminalHeight - 24);
		}
	}
</script>

<div class="flex min-h-0 flex-col bg-card">
	<div class="flex items-center gap-1 px-3 pt-1">
		<!-- Focusable ARIA separator: standard pattern for a resizable splitter handle -->
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="flex flex-1 cursor-ns-resize items-center justify-center py-1 text-muted-foreground hover:text-foreground"
			onpointerdown={onPointerDown}
			onkeydown={onSeparatorKey}
			role="separator"
			aria-orientation="horizontal"
			tabindex={0}
			aria-label="Resize terminal"
		>
			<div class="h-1 w-16 rounded-full bg-border"></div>
		</div>
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
