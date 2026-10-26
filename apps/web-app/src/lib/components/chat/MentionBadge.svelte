<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { cn } from '#lib/ui/cn';
	import { mentionClass, mentionIcon } from '#lib/utils/mention-style';
	import { t } from '#lib/i18n';

	interface Props {
		text: string;
		kind: string;
		surface?: 'card' | 'primary' | 'muted';
		size?: 'micro' | 'caption';
		href?: string | null;
	}

	let { text, kind, surface = 'card', size = 'micro', href = null }: Props = $props();

	const surfaceClass = $derived(
		surface === 'primary'
			? 'border-primary-foreground/30 bg-primary-foreground/10 text-primary-foreground'
			: surface === 'muted'
				? 'border-border/70 bg-muted/60 text-muted-foreground'
				: 'border-border/70 bg-muted/60 text-muted-foreground',
	);

	const classes = $derived(
		cn(
			'inline-flex items-center gap-1 rounded border px-1.5 font-mono not-italic',
			size === 'micro' ? 'text-micro' : 'text-caption',
			surfaceClass,
			mentionClass(kind, text),
			href
				? 'cursor-pointer underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))]'
				: '',
		),
	);
</script>

{#if href}
	<a
		href={href}
		class={classes}
		title={t('chat.mentionBadge.openInIde', { kind, text })}
		aria-label={t('chat.mentionBadge.openInIde', { kind, text })}
	>
		<Icon name={mentionIcon(kind, text)} size={size === 'micro' ? 11 : 12} />
		{text}
	</a>
{:else}
	<span
		class={classes}
		title={text}
		aria-label={t('chat.mentionBadge.aria', { kind, text })}
	>
		<Icon name={mentionIcon(kind, text)} size={size === 'micro' ? 11 : 12} />
		{text}
	</span>
{/if}
