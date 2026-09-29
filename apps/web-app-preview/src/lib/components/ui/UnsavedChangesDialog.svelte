<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';

	interface Props {
		open: boolean;
		title?: string;
		description: string;
		discardLabel?: string;
		saveLabel?: string | null;
		busy?: boolean;
		ondiscard: () => void;
		onsave?: () => void;
		onstay?: () => void;
	}

	let {
		open = $bindable(false),
		title = 'Unsaved changes',
		description,
		discardLabel = 'Discard changes',
		saveLabel = null,
		busy = false,
		ondiscard,
		onsave,
		onstay,
	}: Props = $props();

	function stay(): void {
		open = false;
		onstay?.();
	}
</script>

<Dialog
	bind:open
	{title}
	{description}
	width="26rem"
	onclose={() => onstay?.()}
>
	<p class="text-caption text-muted-foreground">{description}</p>
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" disabled={busy} onclick={stay}>
				Keep editing
			</Button>
			{#if saveLabel}
				<Button
					variant="outline"
					size="sm"
					disabled={busy}
					onclick={() => onsave?.()}
				>
					{busy ? 'Saving…' : saveLabel}
				</Button>
			{/if}
			<Button
				variant="destructive"
				size="sm"
				disabled={busy}
				onclick={() => ondiscard()}
			>
				{discardLabel}
			</Button>
		</div>
	{/snippet}
</Dialog>
