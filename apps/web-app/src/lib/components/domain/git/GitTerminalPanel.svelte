<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { GitWorkbench } from '$lib/git/workbench.svelte';

	let { workbench }: { workbench: GitWorkbench } = $props();
</script>

{#if workbench.terminalOpen}
	<Card
		title="Terminal"
		description="Repo-locked shell; output polls the session buffer."
	>
		<div class="mb-2 flex gap-2">
			<Button size="sm" variant="ghost" onclick={() => workbench.clearTerminal()}
				>Clear</Button
			>
			<Button
				size="sm"
				variant="ghost"
				onclick={() => void workbench.restartTerminal()}>Restart</Button
			>
		</div>
		<pre
			class="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap">{workbench.terminalText ||
				'(empty)'}</pre>
		<div class="mt-2 flex gap-2">
			<Input
				label="Terminal input"
				size="sm"
				bind:value={workbench.terminalInput}
				placeholder="git status --short --branch"
			/>
			<Button size="sm" onclick={() => void workbench.sendTerminal()}
				>Send</Button
			>
		</div>
	</Card>
{/if}
