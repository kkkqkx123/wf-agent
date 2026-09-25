<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Button from '$lib/components/ui/Button.svelte';

	// SvelteKit passes { status, message } when an error bubbles up from load/endpoint.
	// eslint-disable-next-line svelte/valid-prop-names-in-kit-pages
	let { status = 500, message = '' }: { status?: number; message?: string } =
		$props();

	const errorTitle = $derived(
		status === 404 ? 'Page not found' : 'Something went wrong',
	);
	const errorMessage = $derived(
		status === 404
			? 'The page you were looking for does not exist or has been moved.'
			: message || 'An unexpected error occurred.',
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
			<p class="font-mono text-sm tabular-nums text-muted-foreground">
				{status || 500}
			</p>
			<h1 class="mt-2 text-xl font-semibold text-foreground">{errorTitle}</h1>
			<p class="mt-2 text-caption text-muted-foreground">{errorMessage}</p>
			<div class="mt-6 flex items-center justify-center gap-2">
				<Button variant="outline" size="sm" onclick={reload}>Retry</Button>
				<Button variant="outline" size="sm" onclick={goHome}>Go home</Button>
			</div>
		</div>
	</div>
</div>
