<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { t } from '#lib/i18n';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<div
	class="flex flex-wrap items-center gap-x-3 gap-y-1 border border-border bg-card px-2 py-1 font-mono text-micro"
	role="status"
	aria-label={t('domain.git.keyHints')}
>
	<span class="font-semibold tracking-wider text-info uppercase">[{workbench.panel}]</span>
	{#if workbench.selection}
		<span class="max-w-64 truncate text-muted-foreground">{workbench.selection}</span>
	{/if}
	<span class="text-muted-foreground">{workbench.controls.length} actions</span>
	{#if workbench.refreshing}
		<span
			class="flex items-center gap-1 text-muted-foreground"
			role="status"
			title={t('common.refresh')}
		>
			<Icon name="loader" size={12} class="animate-spin" />
		</span>
	{/if}
	<span class="ml-auto flex items-center gap-1">
		<Button
			size="sm"
			variant="ghost"
			title="All actions"
			onclick={() => {
				workbench.helpOpen = true;
			}}
		>
			<span class="text-muted-foreground">?</span> help
		</Button>
		<Button size="sm" variant="ghost" title="Focus filter" onclick={() => workbench.focusFilter()}>
			<span class="text-muted-foreground">/</span> filter
		</Button>
		<Button
			size="sm"
			variant="ghost"
			title="Focus key input"
			onclick={() => workbench.focusKeyInput()}
		>
			<span class="text-muted-foreground">^/</span> keys
		</Button>
	</span>
</div>
