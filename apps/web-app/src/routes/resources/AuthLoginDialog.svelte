<script lang="ts">
	import { onDestroy } from 'svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import { t } from '#lib/i18n';
	import {
		cancelAuthLogin,
		loginPollInterval,
		parseLoginState,
		pollAuthLogin,
		startAuthLogin,
		submitAuthLogin,
		type AuthProviderStatus,
		type ParsedLoginState,
	} from '#lib/services/auth';
	import { toasts } from '#lib/stores/toast.svelte';

	interface Props {
		open?: boolean;
		provider?: AuthProviderStatus | null;
		ondone?: () => void;
	}

	let { open = $bindable(false), provider = null, ondone }: Props = $props();

	let sessionId = $state('');
	let loginState = $state<ParsedLoginState | null>(null);
	let busy = $state(false);
	let input = $state('');
	let selectedOption = $state('');
	let error = $state<string | null>(null);
	let pollTimer = $state<ReturnType<typeof setTimeout> | null>(null);
	let startedFor = $state('');

	function clearTimer(): void {
		if (pollTimer) {
			clearTimeout(pollTimer);
			pollTimer = null;
		}
	}

	function resetDialog(): void {
		clearTimer();
		sessionId = '';
		loginState = null;
		busy = false;
		input = '';
		selectedOption = '';
		error = null;
		startedFor = '';
	}

	function close(): void {
		if (
			sessionId &&
			loginState &&
			loginState.kind !== 'Completed' &&
			loginState.kind !== 'Failed'
		) {
			void cancelAuthLogin(sessionId).catch(() => undefined);
		}
		resetDialog();
		open = false;
	}

	async function startLogin(target: AuthProviderStatus): Promise<void> {
		resetDialog();
		busy = true;
		error = null;
		try {
			const started = await startAuthLogin(target.id);
			sessionId = started.session_id;
			startedFor = target.id;
			loginState = parseLoginState(started.state);
			if (!loginState.prompt && selectedOption === '') {
				selectedOption = '';
			}
			schedulePoll();
			if (loginState.kind === 'Completed') {
				finishCompleted();
			} else if (loginState.kind === 'Failed') {
				error = loginState.error ?? t('page.resources.auth.loginFailed');
			}
		} catch (e) {
			error =
				e instanceof Error ? e.message : t('page.resources.auth.loginFailed');
		} finally {
			busy = false;
		}
	}

	function schedulePoll(): void {
		clearTimer();
		if (!open || !sessionId || !loginState) return;
		if (loginState.kind === 'AwaitingInput') return;
		if (loginState.kind === 'Completed' || loginState.kind === 'Failed') return;
		const delay = loginPollInterval(loginState.events) * 1000;
		pollTimer = setTimeout(() => void pollOnce(), delay);
	}

	async function pollOnce(): Promise<void> {
		if (!sessionId || !open) return;
		try {
			const state = await pollAuthLogin(sessionId);
			loginState = parseLoginState(state);
			if (loginState.kind === 'Completed') {
				finishCompleted();
				return;
			}
			if (loginState.kind === 'Failed') {
				error = loginState.error ?? t('page.resources.auth.loginFailed');
				return;
			}
			schedulePoll();
		} catch (e) {
			error =
				e instanceof Error ? e.message : t('page.resources.auth.loginFailed');
		}
	}

	function finishCompleted(): void {
		clearTimer();
		toasts.success(t('page.resources.auth.loginSucceeded'));
		ondone?.();
		resetDialog();
		open = false;
	}

	async function submitInput(): Promise<void> {
		if (!sessionId || !loginState?.prompt) return;
		const prompt = loginState.prompt;
		const value =
			prompt.kind === 'Select' ? selectedOption.trim() : input.trim();
		if (!value) {
			error = t('page.resources.auth.inputRequired');
			return;
		}
		busy = true;
		error = null;
		try {
			const state = await submitAuthLogin(sessionId, value);
			loginState = parseLoginState(state);
			input = '';
			selectedOption = '';
			if (loginState.kind === 'Completed') {
				finishCompleted();
				return;
			}
			if (loginState.kind === 'Failed') {
				error = loginState.error ?? t('page.resources.auth.loginFailed');
				return;
			}
			schedulePoll();
		} catch (e) {
			error =
				e instanceof Error ? e.message : t('page.resources.auth.loginFailed');
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		if (open && provider && provider.id !== startedFor) {
			void startLogin(provider);
		}
		if (!open) {
			clearTimer();
		}
	});

	onDestroy(() => clearTimer());

	const prompt = $derived(loginState?.prompt ?? null);
	const isSecret = $derived(
		prompt?.kind === 'Secret' || prompt?.kind === 'ManualCode',
	);
</script>

<Dialog
	bind:open
	title={provider
		? t('page.resources.auth.connectTitle', {
				name: provider.display_name || provider.id,
			})
		: t('page.resources.auth.title')}
	description={provider?.login_label ?? undefined}
	onclose={close}
>
	{#if error}
		<p
			class="mb-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive"
		>
			{error}
		</p>
	{/if}

	{#if !loginState}
		<p class="text-caption text-muted-foreground">
			{busy
				? t('page.resources.auth.starting')
				: t('page.resources.auth.emptyDesc')}
		</p>
	{:else}
		<div class="space-y-2">
			{#each loginState.events as event (event)}
				{#if event.kind === 'AuthUrl'}
					<div class="rounded-md border border-border bg-muted/40 p-2">
						<p class="text-caption text-muted-foreground">
							{event.instructions ?? t('page.resources.auth.openUrl')}
						</p>
						<a
							href={event.url}
							target="_blank"
							rel="noreferrer"
							class="mt-1 block truncate font-mono text-small text-primary underline"
						>
							{event.url}
						</a>
					</div>
				{:else if event.kind === 'DeviceCode'}
					<div class="rounded-md border border-border bg-muted/40 p-2">
						<p class="text-caption text-muted-foreground">
							{t('page.resources.auth.visitUrl')}
						</p>
						<a
							href={event.verification_uri}
							target="_blank"
							rel="noreferrer"
							class="mt-1 block truncate font-mono text-small text-primary underline"
						>
							{event.verification_uri}
						</a>
						<p class="mt-1 font-mono text-title font-semibold tracking-widest">
							{event.user_code}
						</p>
					</div>
				{:else if event.kind === 'Info'}
					<div class="rounded-md border border-border p-2">
						<p class="text-caption">{event.message}</p>
						{#if event.links.length > 0}
							<div class="mt-1 space-y-1">
								{#each event.links as link (link.url)}
									<a
										href={link.url}
										target="_blank"
										rel="noreferrer"
										class="block truncate text-small text-primary underline"
									>
										{link.label ?? link.url}
									</a>
								{/each}
							</div>
						{/if}
					</div>
				{:else if event.kind === 'Progress'}
					<p class="text-caption text-muted-foreground">{event.message}</p>
				{/if}
			{/each}

			{#if loginState.kind === 'AwaitingInput' && prompt}
				<div class="rounded-md border border-border p-2">
					<p class="mb-2 text-caption font-medium">{prompt.message}</p>
					{#if prompt.kind === 'Select'}
						<Select
							bind:value={selectedOption}
							options={prompt.options.map((option) => ({
								value: option.id,
								label: option.description
									? `${option.label} · ${option.description}`
									: option.label,
							}))}
							placeholder={prompt.message}
							class="w-full"
						/>
					{:else}
						<form
							onsubmit={(event) => {
								event.preventDefault();
								void submitInput();
							}}
						>
							<Input
								bind:value={input}
								type={isSecret ? 'password' : 'text'}
								placeholder={prompt.placeholder ?? prompt.message}
								autocomplete="off"
								spellcheck={false}
							/>
						</form>
					{/if}
				</div>
			{:else if loginState.kind === 'Completed'}
				<p class="text-caption text-success">
					{t('page.resources.auth.loginSucceeded')}
				</p>
			{:else if loginState.kind === 'Failed'}
				<p class="text-caption text-destructive">
					{loginState.error ?? t('page.resources.auth.loginFailed')}
				</p>
			{:else}
				<p class="text-caption text-muted-foreground">
					{t('page.resources.auth.waiting')}
				</p>
			{/if}
		</div>
	{/if}

	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="outline" size="sm" onclick={close}>
				{t('page.resources.auth.cancel')}
			</Button>
			{#if loginState?.kind === 'Failed'}
				<Button
					size="sm"
					disabled={busy || !provider}
					onclick={() => provider && void startLogin(provider)}
				>
					{t('page.resources.auth.retry')}
				</Button>
			{:else if prompt}
				<Button size="sm" disabled={busy} onclick={() => void submitInput()}>
					{busy
						? t('page.resources.auth.submitting')
						: t('page.resources.auth.submit')}
				</Button>
			{/if}
		</div>
	{/snippet}
</Dialog>
