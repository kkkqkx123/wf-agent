<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import type { Tool } from '$lib/types/models';
	import { formatNumber, formatPercent } from '$lib/utils/format';

	interface Props {
		tools: Tool[];
		toolEnabled: Record<string, boolean>;
		ontoggle: (id: string, checked: boolean) => void;
		onrun: (tool: Tool) => void;
	}

	let { tools, toolEnabled, ontoggle, onrun }: Props = $props();
</script>

{#if tools.length === 0}
	<EmptyState
		icon="blocks"
		title="No tools registered"
		description="Tools appear here once the registry has entries."
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
		{#each tools as tool (tool.id)}
			<Card title={tool.name}>
				{#snippet actions()}
					<Switch
						checked={toolEnabled[tool.id] ?? false}
						label="Enable {tool.name}"
						hideLabel
						onchange={(checked) => ontoggle(tool.id, checked)}
					/>
				{/snippet}
				<p class="text-caption text-muted-foreground">
					{tool.description}
				</p>
				<div
					class="mt-2 flex items-center justify-between text-micro text-muted-foreground"
				>
					<span class="rounded border border-border px-1.5 py-0.5"
						>{tool.kind}</span
					>
					<span class="tabular-nums">
						{formatNumber(tool.calls)} calls ·
						{tool.successRate === null
							? '—'
							: formatPercent(tool.successRate, 0)} ok
					</span>
				</div>
				{#snippet footer()}
					<div class="flex items-center gap-2">
						<Button variant="ghost" size="sm" onclick={() => onrun(tool)}>
							Validate / Run
						</Button>
					</div>
				{/snippet}
			</Card>
		{/each}
	</div>
{/if}
