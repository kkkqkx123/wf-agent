<script lang="ts">
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<Card
	title={t('domain.git.keyInputTitle')}
	description={t('domain.git.keyInputDescription')}
>
	<div class="flex gap-2">
		<Input
			id="git-input"
			label={t('domain.git.keyLabel')}
			size="sm"
			bind:value={workbench.token}
			placeholder="c commit · space stage · ? all"
			onkeydown={(event) => {
				if (event.key === 'Enter') void workbench.submitToken();
			}}
		/>
		<Button
			size="sm"
			disabled={gitStore.busy}
			onclick={() => void workbench.submitToken()}>{t('domain.git.run')}</Button
		>
	</div>
	{#if workbench.tokenResult.kind === 'action'}
		<p class="mt-1 text-caption">
			{t('domain.git.willRun')}
			<span class="font-mono">{workbench.tokenResult.action}</span>
			{#if workbench.tokenResult.danger !== 'readonly' && workbench.tokenResult.danger !== 'recoverable'}
				<Badge variant="danger">{workbench.tokenResult.danger}</Badge>
			{/if}
		</p>
	{:else if workbench.tokenResult.kind === 'prefix' && workbench.tokenResult.candidates?.length}
		<p class="mt-1 text-micro text-muted-foreground">
			{workbench.tokenResult.candidates?.slice(0, 8).join(', ')}
		</p>
	{:else if workbench.tokenResult.kind === 'help' && workbench.tokenResult.candidates?.length}
		<p class="mt-1 text-micro text-muted-foreground">
			{workbench.tokenResult.candidates?.join(', ')}
		</p>
	{/if}
	{#if gitStore.actionError}
		<p class="mt-1 text-caption text-destructive">
			{gitStore.actionError}
		</p>
	{/if}
</Card>
