<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';
	import type { StarterSuggestion } from '#lib/services/starters';

	interface Props {
		suggestions: StarterSuggestion[];
		onSelect?: (suggestion: StarterSuggestion) => void;
	}

	const STORAGE_KEY = 'wf.chat.composer.presetRows';
	const ROW_OPTIONS = [1, 2, 3] as const;

	let { suggestions, onSelect }: Props = $props();

	let rows = $state<number>(2);
	let expanded = $state(false);

	$effect(() => {
		try {
			const raw = localStorage.getItem(STORAGE_KEY);
			const parsed = raw === null ? NaN : Number.parseInt(raw, 10);
			if (parsed === 1 || parsed === 2 || parsed === 3) rows = parsed;
		} catch {
			rows = 2;
		}
	});

	$effect(() => {
		void suggestions.length;
		expanded = false;
	});

	function setRows(next: number): void {
		rows = next;
		expanded = false;
		try {
			localStorage.setItem(STORAGE_KEY, String(next));
		} catch {
			// Storage may be unavailable; the in-memory choice still applies.
		}
	}

	const collapsedClass = $derived(
		rows === 1 ? 'max-h-9 overflow-hidden' : rows === 2 ? 'max-h-[4.5rem] overflow-hidden' : '',
	);
</script>

{#if suggestions.length > 0}
	<div class="flex flex-col gap-1.5">
		<div class="flex items-center justify-between gap-2">
			<p class="sr-only">{t('chat.composer.presetRows')}</p>
			<div class="flex items-center gap-1" role="group" aria-label={t('chat.composer.presetRows')}>
				{#each ROW_OPTIONS as option (option)}
					<button
						type="button"
						aria-pressed={rows === option && !expanded}
						onclick={() => setRows(option)}
						class={cn(
							'rounded border border-border px-1.5 py-0.5 font-mono text-micro text-muted-foreground transition-colors hover:bg-accent hover:text-foreground',
							rows === option && !expanded && 'border-ring bg-accent text-foreground',
						)}
					>
						{t('chat.composer.presetRowsOption', { count: option })}
					</button>
				{/each}
			</div>
		</div>
		<div class={cn('flex flex-wrap gap-1.5', !expanded && collapsedClass)}>
			{#each suggestions as suggestion (suggestion.id ?? suggestion.title)}
				<button
					type="button"
					title={suggestion.title}
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
