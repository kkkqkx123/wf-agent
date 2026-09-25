<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconButton from './IconButton.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		open: boolean;
		title: string;
		description?: string;
		width?: string;
		class?: string;
		onclose?: () => void;
		/** Confirmation dialogs carry everything in title and description. */
		children?: Snippet;
		footer?: Snippet;
	}

	let {
		open = $bindable(false),
		title,
		description,
		width = '32rem',
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
		if (event.key === 'Escape' && open) {
			event.stopPropagation();
			close();
		}
	}
</script>

<svelte:window {onkeydown} />

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<!-- Backdrop uses the restrained frost reserved for overlays. -->
		<button
			type="button"
			aria-label="Close dialog"
			class="animate-overlay-in absolute inset-0 bg-overlay backdrop-blur-[2px]"
			onclick={close}
		></button>
		<div
			role="dialog"
			aria-modal="true"
			aria-label={title}
			style:width
			class={cn(
				'animate-panel-in relative max-h-[85vh] w-full overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-popover',
				className,
			)}
		>
			<header
				class="flex items-start justify-between gap-3 border-b border-border px-4 py-3"
			>
				<div class="min-w-0">
					<h2 class="text-title font-semibold">{title}</h2>
					{#if description}
						<p class="mt-0.5 text-caption text-muted-foreground">
							{description}
						</p>
					{/if}
				</div>
				<IconButton icon="x" label="Close" compact onclick={close} />
			</header>
			<div class="max-h-[60vh] overflow-y-auto px-4 py-3">
				{#if children}
					{@render children()}
				{/if}
			</div>
			{#if footer}
				<footer
					class="flex items-center justify-end gap-2 border-t border-border px-4 py-3"
				>
					{@render footer()}
				</footer>
			{/if}
		</div>
	</div>
{/if}
