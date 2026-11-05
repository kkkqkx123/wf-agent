<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { dev } from '$app/env';
	import Button from '#lib/ui/components/Button.svelte';
	import { t } from '#lib/i18n';

	// SvelteKit renders the error page with layout props only; the live page
	// state carries the real status and error. Destructuring them from $props()
	// would always fall back to 500 and hide the actual message.
	const status = $derived(page.status);
	const message = $derived(page.error?.message ?? '');
	const stack = $derived(
		page.error && typeof page.error === 'object'
			? (page.error as { stack?: unknown }).stack
			: undefined,
	);

	const errorTitle = $derived(t(status === 404 ? 'page.error.title404' : 'page.error.title500'));
	const errorMessage = $derived(
		status === 404 ? t('page.error.desc404') : message || t('page.error.desc500'),
	);

	function reload(): void {
		const url = new URL(window.location.href);
		url.searchParams.set('_retry', String(Date.now()));
		window.location.href = url.toString();
	}

	function goHome(): void {
		void goto(resolve('/executions'));
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex flex-1 items-center justify-center px-4 py-12">
		<div class="max-w-md text-center">
			<p class="font-mono text-caption tabular-nums text-muted-foreground">
				{status || 500}
			</p>
			<h1 class="mt-2 text-title font-semibold text-foreground">{errorTitle}</h1>
			<p class="mt-2 text-caption text-muted-foreground">{errorMessage}</p>
			{#if dev && stack}
				<details class="mt-4 text-left">
					<summary class="cursor-pointer text-micro text-muted-foreground">stack</summary>
					<pre
						class="mt-2 max-h-64 overflow-auto rounded-md border border-border bg-muted/40 p-2 text-left font-mono text-micro text-muted-foreground">{String(
							stack,
						)}</pre>
				</details>
			{/if}
			<div class="mt-6 flex items-center justify-center gap-2">
				<Button variant="outline" size="sm" onclick={reload}>{t('common.retry')}</Button>
				<Button variant="outline" size="sm" onclick={goHome}>{t('page.error.goHome')}</Button>
			</div>
		</div>
	</div>
</div>
