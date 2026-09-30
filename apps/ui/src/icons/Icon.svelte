<script lang="ts">
	import { ICON_PATHS, type IconName } from './paths';

	interface Props {
		name: IconName;
		size?: number;
		strokeWidth?: number;
		class?: string;
	}

	let {
		name,
		size = 16,
		strokeWidth = 1.5,
		class: className = '',
	}: Props = $props();

	const paths = $derived(ICON_PATHS[name] ?? []);
</script>

<svg
	viewBox="0 0 24 24"
	width={size}
	height={size}
	fill="none"
	stroke="currentColor"
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
	class={className}
>
	{#each paths as d (d)}
		<path {d} />
	{/each}
</svg>
