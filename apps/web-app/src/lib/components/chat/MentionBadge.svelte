<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { mentionClass, mentionIcon } from '$lib/utils/mention-style';

	interface Props {
		text: string;
		kind: string;
		surface?: 'card' | 'primary' | 'muted';
		size?: 'micro' | 'caption';
	}

	let {
		text,
		kind,
		surface = 'card',
		size = 'micro',
	}: Props = $props();

	const surfaceClass = $derived(
		surface === 'primary'
			? 'border-primary-foreground/30 bg-primary-foreground/10 text-primary-foreground'
			: surface === 'muted'
				? 'border-border/70 bg-muted/60 text-muted-foreground'
				: 'border-border/70 bg-muted/60 text-muted-foreground',
	);
</script>

<span
	class={cn(
		'inline-flex items-center gap-1 rounded border px-1.5 font-mono not-italic',
		size === 'micro' ? 'text-micro' : 'text-caption',
		surfaceClass,
		mentionClass(kind),
	)}
	title={text}
	aria-label={`Mention ${kind}: ${text}`}
>
	<Icon name={mentionIcon(kind, text)} size={size === 'micro' ? 11 : 12} />
	{text}
</span>
