<script lang="ts">
	import type { TemplateIssue } from '$lib/services/templates';

	interface Props {
		issues: TemplateIssue[];
		onlocate?: (issue: TemplateIssue) => void;
		locatable?: (issue: TemplateIssue) => boolean;
	}

	let {
		issues,
		onlocate,
		locatable = (issue) => issue.nodeId !== null,
	}: Props = $props();
</script>

{#if issues.length > 0}
	<ul
		class="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
	>
		{#each issues as issue, index (`${issue.source}-${issue.field ?? ''}-${issue.message}-${index}`)}
			<li
				class="flex items-center justify-between gap-2 text-caption text-destructive"
			>
				<span class="min-w-0 truncate">
					<span
						class="mr-1 rounded border border-destructive/40 px-1 text-micro uppercase"
						>{issue.source}</span
					>
					{#if issue.field}<span class="font-mono">{issue.field}</span>:
					{/if}{issue.message}
				</span>
				{#if onlocate && locatable(issue)}
					<button
						type="button"
						class="shrink-0 underline-offset-2 hover:underline"
						onclick={() => onlocate?.(issue)}
					>
						Locate
					</button>
				{/if}
			</li>
		{/each}
	</ul>
{/if}
