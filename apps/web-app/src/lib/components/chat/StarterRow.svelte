<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';
	import type { StarterSuggestion } from '#lib/services/starters';

	interface Props {
		suggestions: StarterSuggestion[];
		onSelect?: (suggestion: StarterSuggestion) => void;
	}

	let { suggestions, onSelect }: Props = $props();

	let expanded = $state(false);

	$effect(() => {
		void suggestions;
		expanded = false;
	});
</script>

{#if suggestions.length > 0}
	<div class="flex flex-col gap-1.5">
		<div
			class={cn(
				'flex flex-wrap gap-1.5',
				expanded
					? 'max-h-40 overflow-y-auto'
					: 'max-h-[4.5rem] overflow-hidden [mask-image:linear-gradient(to_bottom,black_70%,transparent)]',
			)}
		>
			{#each suggestions as suggestion (suggestion.id ?? suggestion.title)}
				<button
					type="button"
					onclick={() => onSelect?.(suggestion)}
					class="rounded-full border border-border bg-muted/50 px-2.5 py-1 text-caption text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				>
					{suggestion.title}
				</button>
			{/each}
		</div>
		{#if suggestions.length > 4}
			<button
				type="button"
				aria-expanded={expanded}
				onclick={() => (expanded = !expanded)}
				class="inline-flex w-fit items-center gap-1 px-0.5 text-micro text-info hover:underline"
			>
				<Icon name={expanded ? 'chevron-up' : 'chevron-down'} size={12} />
				{expanded
					? t('chat.composer.showLessSuggestions')
					: t('chat.composer.showAllSuggestions', { count: suggestions.length })}
			</button>
		{/if}
	</div>
{/if}
