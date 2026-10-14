<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { git as gitStore } from '$lib/stores/git.svelte';
	import type { GitWorkbench } from '$lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<Card
	title="Key input"
	description="Action-first; single chars are aliases. : forwards to terminal, ? lists actions."
>
	<div class="flex gap-2">
		<Input
			id="git-input"
			label="Git key"
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
			onclick={() => void workbench.submitToken()}>Run</Button
		>
	</div>
	{#if workbench.tokenResult.kind === 'action'}
		<p class="mt-1 text-caption">
			Will run <span class="font-mono">{workbench.tokenResult.action}</span>
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
