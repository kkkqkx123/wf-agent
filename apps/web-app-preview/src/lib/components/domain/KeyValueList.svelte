<script lang="ts">
	import type { KeyValue } from '$lib/types/models';
	import { cn } from '$lib/utils/cn';

	interface Props {
		items: KeyValue[];
		dense?: boolean;
		mono?: boolean;
		class?: string;
	}

	let {
		items,
		dense = false,
		mono = true,
		class: className = '',
	}: Props = $props();
</script>

<dl
	class={cn(
		'grid grid-cols-[minmax(6rem,auto)_1fr] gap-x-4',
		dense ? 'gap-y-1' : 'gap-y-2',
		className,
	)}
>
	{#each items as item (item.key)}
		<dt class="truncate text-caption text-muted-foreground">{item.key}</dt>
		<dd
			class={cn(
				'min-w-0 break-words text-body text-foreground',
				mono && 'font-mono text-caption',
			)}
		>
			{item.value}
		</dd>
	{/each}
</dl>
