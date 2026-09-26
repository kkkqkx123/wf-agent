<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconButton from './IconButton.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		open: boolean;
		title?: string;
		side?: 'right' | 'bottom' | 'left';
		width?: string;
		height?: string;
		class?: string;
		onclose?: () => void;
		children: Snippet;
		footer?: Snippet;
	}

	let {
		open = $bindable(false),
		title,
		side = 'right',
		width = '26rem',
		height = '70vh',
		class: className = '',
		onclose,
		children,
		footer,
	}: Props = $props();

	function close(): void {
		open = false;
		onclose?.();
	}

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape' && open) close();
	}

	const panelGeometry = $derived(
		side === 'bottom'
			? `left:0;right:0;bottom:0;height:${height};`
			: side === 'left'
				? `left:0;top:0;bottom:0;width:${width};`
				: `right:0;top:0;bottom:0;width:${width};`,
	);
</script>

<svelte:window {onkeydown} />

{#if open}
	<div class="fixed inset-0 z-50">
		<button
			type="button"
			aria-label="Close panel"
			class="animate-overlay-in absolute inset-0 bg-[hsl(var(--overlay))]"
			onclick={close}
		></button>
		<div
			role="dialog"
			aria-modal="true"
			aria-label={title ?? 'Panel'}
			style={panelGeometry}
			class={cn(
				'animate-panel-in absolute flex flex-col border-border bg-popover text-popover-foreground shadow-popover',
				side === 'bottom'
					? 'border-t'
					: side === 'left'
						? 'border-r'
						: 'border-l',
				className,
			)}
		>
			{#if title}
				<header
					class="flex items-center justify-between gap-2 border-b border-border px-4 py-2.5"
				>
					<h2 class="truncate text-title font-semibold">{title}</h2>
					<IconButton icon="x" label="Close" compact onclick={close} />
				</header>
			{/if}
			<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
				{@render children()}
			</div>
			{#if footer}
				<footer
					class="flex items-center justify-end gap-2 border-t border-border px-4 py-2.5"
				>
					{@render footer()}
				</footer>
			{/if}
		</div>
	</div>
{/if}
