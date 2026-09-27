<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		value: unknown;
		collapsed?: boolean;
		maxLength?: number;
		class?: string;
	}

	let {
		value,
		collapsed = false,
		maxLength = 8000,
		class: className = '',
	}: Props = $props();

	let open = $state(!collapsed);

	function renderable(input: unknown): string {
		if (typeof input === 'string') {
			const text = input.trim();
			if (text.startsWith('{') || text.startsWith('[')) {
				try {
					return JSON.stringify(JSON.parse(text), null, 2);
				} catch {
					return input;
				}
			}
			return input;
		}
		if (input === null || input === undefined) return '';
		return JSON.stringify(input, null, 2);
	}

	const text = $derived(renderable(value));
	const truncated = $derived(text.length > maxLength);
	const shown = $derived(truncated ? text.slice(0, maxLength) : text);
</script>

{#if text}
	<div class={cn('min-w-0', className)}>
		<button
			type="button"
			onclick={() => (open = !open)}
			aria-expanded={open}
			class="mb-1 flex items-center gap-1 text-micro text-muted-foreground transition-colors hover:text-foreground"
		>
			<Icon
				name="chevron-down"
				size={12}
				class={cn('transition-transform duration-150', open && 'rotate-180')}
			/>
			{open ? 'Collapse' : 'Expand'}
			{#if truncated}
				<span class="font-mono tabular-nums">
					{shown.length}/{text.length}
				</span>
			{/if}
		</button>
		{#if open}
			<pre
				class="max-h-48 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro break-words whitespace-pre-wrap text-foreground">{shown}{#if truncated}<span class="text-muted-foreground">… truncated</span>{/if}</pre>
		{/if}
	</div>
{:else}
	<p class="font-mono text-micro text-muted-foreground">No content recorded</p>
{/if}
