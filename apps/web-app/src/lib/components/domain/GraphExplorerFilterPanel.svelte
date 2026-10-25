<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import type { GraphFilterState } from '#lib/graph/use-graph-filter.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		filter: GraphFilterState;
		kinds: string[];
		matchLabel: string;
		matchCount: number;
		ontogglekind?: (kind: string) => void;
		onstepmatch?: (delta: number) => void;
	}

	let { filter, kinds, matchLabel, matchCount, ontogglekind, onstepmatch }: Props = $props();
</script>

<Card title={t('domain.graph.filters')} class="shrink-0">
	<div class="flex flex-wrap items-center gap-2">
		<Input
			bind:value={filter.query}
			placeholder={t('domain.graph.searchPlaceholder')}
			onkeydown={(event) => {
				if (event.key === 'Enter') {
					event.preventDefault();
					onstepmatch?.(event.shiftKey ? -1 : 1);
				}
			}}
			class="h-7 w-44"
		/>
		{#if filter.query.trim()}
			<span class="text-micro text-muted-foreground">{matchLabel}</span>
			<IconButton
				icon="chevron-left"
				label={t('domain.graph.previousMatch')}
				disabled={matchCount === 0}
				onclick={() => onstepmatch?.(-1)}
			/>
			<IconButton
				icon="chevron-right"
				label={t('domain.graph.nextMatch')}
				disabled={matchCount === 0}
				onclick={() => onstepmatch?.(1)}
			/>
		{/if}
		{#each kinds as kind (kind)}
			<label
				class="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-2 py-1 text-caption"
			>
				<input
					type="checkbox"
					checked={!filter.hiddenKinds.includes(kind)}
					onchange={() => ontogglekind?.(kind)}
					class="accent-current"
				/>
				<span class="font-mono">{kind}</span>
			</label>
		{/each}
		{#if filter.hiddenKinds.length > 0 || filter.query}
			<Button
				variant="ghost"
				size="sm"
				onclick={() => {
					filter.clear();
				}}
			>
				{t('domain.inspector.clearFilter')}
			</Button>
		{/if}
	</div>
</Card>
