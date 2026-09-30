<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import type { Skill } from '$lib/types/models';
	import { cn } from '$lib/utils/cn';

	interface Props {
		skills: Skill[];
		skillEnabled: Record<string, boolean>;
		ontoggle: (id: string, checked: boolean) => void;
		onview: (skill: Skill) => void;
	}

	let { skills, skillEnabled, ontoggle, onview }: Props = $props();
</script>

{#if skills.length === 0}
	<EmptyState
		icon="sparkles"
		title="No skills registered"
		description="Skills appear here once the registry has entries."
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
		{#each skills as skill (skill.id)}
			<Card title={skill.name}>
				{#snippet actions()}
					<Switch
						checked={skillEnabled[skill.id] ?? false}
						label="Enable {skill.name}"
						hideLabel
						onchange={(checked) => ontoggle(skill.id, checked)}
					/>
				{/snippet}
				<p class="text-caption text-muted-foreground">
					{skill.description}
				</p>
				<p
					class="mt-2 rounded-md bg-muted px-2 py-1.5 font-mono text-micro text-muted-foreground"
				>
					{skill.promptPreview}
				</p>
				{#snippet footer()}
					<div class="flex items-center justify-between">
						<span
							class={cn(
								'tabular-nums',
								skill.enabled ? 'text-success' : 'text-muted-foreground',
							)}
						>
							v{skill.version}
						</span>
						<Button variant="ghost" size="sm" onclick={() => onview(skill)}>
							View prompt
						</Button>
					</div>
				{/snippet}
			</Card>
		{/each}
	</div>
{/if}
