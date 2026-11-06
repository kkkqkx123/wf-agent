<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { startPtyKeepalive, stopPtyKeepalive } from '#lib/services/pty-keepalive';

	// Layout-level terminal host: keeps headless connections for sessions
	// without a visible view so route switches only replay a small delta.
	// It renders nothing visible; visible views stay owned by each panel.
	onMount(() => {
		startPtyKeepalive();
	});

	onDestroy(() => {
		stopPtyKeepalive();
	});
</script>

<div class="hidden" aria-hidden="true" data-testid="pty-host"></div>
