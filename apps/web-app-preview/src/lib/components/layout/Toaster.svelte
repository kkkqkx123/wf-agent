<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { cn } from '$lib/utils/cn';

	const TONE_ICON: Record<string, IconName> = {
		success: 'check-circle',
		danger: 'alert-circle',
		warning: 'alert-triangle',
		info: 'info',
	};

	const TONE_CLASS: Record<string, string> = {
		success: 'text-success',
		danger: 'text-destructive',
		warning: 'text-warning',
		info: 'text-info',
	};
</script>

{#if toasts.items.length > 0}
	<div
		class="pointer-events-none fixed bottom-3 right-3 z-[60] flex w-80 flex-col gap-2"
		role="region"
		aria-label="Notifications"
	>
		{#each toasts.items as toast (toast.id)}
			<div
				class="animate-toast-in pointer-events-auto flex items-start gap-2 rounded-lg border border-border bg-popover px-3 py-2.5 shadow-popover"
				role="status"
			>
				<Icon
					name={TONE_ICON[toast.tone] ?? 'info'}
					size={15}
					class={cn('mt-0.5 shrink-0', TONE_CLASS[toast.tone] ?? 'text-info')}
				/>
				<div class="min-w-0 flex-1">
					<p class="text-body font-medium text-foreground">{toast.title}</p>
					{#if toast.description}
						<p class="mt-0.5 text-caption text-muted-foreground">
							{toast.description}
						</p>
					{/if}
					{#if toast.action}
						<button
							type="button"
							onclick={() => {
								toast.action?.run();
								toasts.dismiss(toast.id);
							}}
							class="mt-1.5 text-caption font-medium text-info hover:underline"
						>
							{toast.action.label}
						</button>
					{/if}
				</div>
				<IconButton
					icon="x"
					label="Dismiss"
					compact
					onclick={() => toasts.dismiss(toast.id)}
				/>
			</div>
		{/each}
	</div>
{/if}
