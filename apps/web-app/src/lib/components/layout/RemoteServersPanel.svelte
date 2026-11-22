<script lang="ts">
	import { onMount } from 'svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import { t } from '#lib/i18n';
	import { toasts } from '#lib/stores/toast.svelte';
	import {
		applyBackendDeploy,
		fetchDeploySources,
		listBackends,
		planBackendDeploy,
		probeBackendDeploy,
		type BackendRow,
		type DeployAction,
		type DeployJob,
		type DeployOutcome,
		type DeployPlan,
		type DeployProbe,
		type DeployRequestPayload,
		type DeployScope,
		type DeploySource,
	} from '#lib/services/backends';

	/**
	 * Remote deployment panel.
	 *
	 * The browser never talks to a host: every action goes to the connected
	 * server, which owns the SSH channel, the artifact download and the pid
	 * bookkeeping. That is why this panel holds no credentials, no config
	 * text and no desired state — the same request body drives the CLI.
	 *
	 * Dry run is the default. "Apply" is only enabled after a plan came back
	 * without a blocking reason, so an operator always sees the command
	 * sequence before it runs. `Probe` is the standalone verification verb.
	 */
	const ACTIONS: DeployAction[] = ['probe', 'install', 'start', 'stop', 'upgrade', 'uninstall'];
	const SCOPES: DeployScope[] = ['none', 'config', 'full'];

	const ACTION_LABEL: Record<DeployAction, () => string> = {
		probe: () => t('page.settings.remoteActionProbe'),
		install: () => t('page.settings.remoteActionInstall'),
		start: () => t('page.settings.remoteActionStart'),
		stop: () => t('page.settings.remoteActionStop'),
		upgrade: () => t('page.settings.remoteActionUpgrade'),
		uninstall: () => t('page.settings.remoteActionUninstall'),
	};

	const SCOPE_LABEL: Record<DeployScope, () => string> = {
		none: () => t('page.settings.remoteScopeNone'),
		config: () => t('page.settings.remoteScopeConfig'),
		full: () => t('page.settings.remoteScopeFull'),
	};

	let rows = $state<BackendRow[] | null>(null);
	let sources = $state<DeploySource[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let selected = $state('');
	let action = $state<DeployAction>('probe');
	let scope = $state<DeployScope>('none');
	let source = $state('');
	let artifactKind = $state<'release' | 'local'>('release');
	let artifactPath = $state('');
	let busy = $state(false);
	let plan = $state<DeployPlan | null>(null);
	let outcome = $state<DeployOutcome | null>(null);
	let probe = $state<DeployProbe | null>(null);
	let deployJob = $state<DeployJob | null>(null);

	const actionOptions = $derived(
		ACTIONS.map((value) => ({ value, label: ACTION_LABEL[value] })),
	);
	const scopeOptions = $derived(
		SCOPES.map((value) => ({ value, label: SCOPE_LABEL[value] })),
	);
	const sourceOptions = $derived([
		{ value: '', label: t('page.settings.remoteSourceRoot') },
		...sources.map((item) => ({
			value: item.name,
			label: item.name || item.path,
		})),
	]);
	const artifactOptions = $derived([
		{ value: 'release', label: t('page.settings.remoteArtifactRelease') },
		{ value: 'local', label: t('page.settings.remoteArtifactLocal') },
	]);
	const hostRows = $derived(rows ?? []);
	const selectedSource = $derived(sources.find((item) => item.name === source) ?? null);
	const transfersFiles = $derived(action === 'install' || action === 'upgrade');

	function resetResults(): void {
		plan = null;
		outcome = null;
	}

	function buildPayload(): DeployRequestPayload {
		const payload: DeployRequestPayload = {
			action,
			scope,
			source: source || null,
		};
		if (action === 'install' || action === 'upgrade') {
			payload.artifact =
				artifactKind === 'local'
					? { kind: 'local', path: artifactPath.trim() || undefined }
					: { kind: 'release' };
		}
		return payload;
	}

	function describeFailure(e: unknown): string {
		return e instanceof Error ? e.message : String(e);
	}

	async function load(): Promise<void> {
		loading = true;
		error = null;
		try {
			rows = await listBackends();
			if (rows && rows.length > 0 && !selected) selected = rows[0].name;
			if (selected) {
				try {
					sources = await fetchDeploySources(selected);
				} catch {
					// Source discovery is local; a disabled backend domain has
					// no deploy status route and simply yields no choices.
					sources = [];
				}
			}
		} catch (e) {
			error = describeFailure(e);
		} finally {
			loading = false;
		}
	}

	async function runProbe(): Promise<void> {
		if (!selected) return;
		busy = true;
		resetResults();
		try {
			probe = await probeBackendDeploy(selected);
		} catch (e) {
			toasts.error(describeFailure(e));
		} finally {
			busy = false;
		}
	}

	async function runPlan(): Promise<void> {
		if (!selected) return;
		busy = true;
		outcome = null;
		try {
			plan = await planBackendDeploy(selected, buildPayload());
		} catch (e) {
			toasts.error(describeFailure(e));
		} finally {
			busy = false;
		}
	}

	async function runApply(): Promise<void> {
		if (!selected) return;
		busy = true;
		deployJob = null;
		try {
			outcome = await applyBackendDeploy(selected, buildPayload(), (job) => {
				deployJob = job;
			});
			if (outcome.ok) {
				toasts.success(t('page.settings.remoteApplied'));
			} else {
				toasts.error(outcome.error ?? t('page.settings.remoteApplyFailed'));
			}
			plan = null;
		} catch (e) {
			toasts.error(describeFailure(e));
		} finally {
			busy = false;
		}
	}

	onMount(() => {
		void load();
	});
</script>

<Card
	title={t('page.settings.remoteServers')}
	description={t('page.settings.remoteServersDesc')}
>
	<div class="space-y-4 px-4 py-3">
		{#if loading}
			<p class="text-body text-muted-foreground">{t('page.settings.remoteLoading')}</p>
		{:else if error}
			<p class="text-body text-destructive">{error}</p>
		{:else if rows === null}
			<p class="text-body text-muted-foreground">{t('page.settings.remoteDisabled')}</p>
		{:else if hostRows.length === 0}
			<p class="text-body text-muted-foreground">{t('page.settings.remoteEmpty')}</p>
		{:else}
			<div class="flex flex-wrap items-center gap-2">
				<Select
					class="w-60"
					value={selected}
					options={hostRows.map((row) => ({ value: row.name, label: row.name }))}
					onchange={(value) => {
						selected = value;
						probe = null;
						resetResults();
						void fetchDeploySources(value)
							.then((found) => (sources = found))
							.catch(() => (sources = []));
					}}
				/>
				<Select
					class="w-44"
					value={action}
					options={actionOptions}
					onchange={(value) => {
						action = value as DeployAction;
						resetResults();
					}}
				/>
				<Button size="sm" variant="ghost" disabled={busy} onclick={() => void runProbe()}>
					{t('page.settings.remoteProbe')}
				</Button>
			</div>

			{#if probe}
				<div class="space-y-2 rounded-md border border-border bg-card/40 p-3 text-small">
					<div class="flex flex-wrap items-center gap-2">
						<span class="font-medium">{t('page.settings.remoteVerdict')}:</span>
						<span class="font-mono" class:text-destructive={probe.verdict !== 'ok'}>
							{probe.verdict || '—'}
						</span>
						{#if probe.target}
							<span class="text-muted-foreground">
								{t('page.settings.remotePlatform')}: <span class="font-mono">{probe.target.platform}</span>
								· {t('page.settings.remoteHome')}: <span class="font-mono">{probe.target.home}</span>
							</span>
						{/if}
					</div>
					{#if probe.error}
						<p class="text-destructive">{probe.error}</p>
					{/if}
					{#if probe.installation}
						<p class="text-muted-foreground">
							{t('page.settings.remoteInstalled')}:
							<span class="font-mono">
								{probe.installation.version ?? '—'} · {probe.installation.running
									? t('page.settings.remoteRunning')
									: t('page.settings.remoteStopped')}
								{#if probe.installation.healthy}· {t('page.settings.remoteHealthy')}{/if}
							</span>
						</p>
					{/if}
					{#if probe.checks.length > 0}
						<ul class="space-y-1">
							{#each probe.checks as check}
								<li class="flex gap-2">
									<span class="font-mono" class:text-destructive={!check.ok}>
										{check.ok ? 'ok' : 'fail'}
									</span>
									<span class="font-mono">{check.id}</span>
									<span class="text-muted-foreground">{check.detail}</span>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{/if}

			{#if transfersFiles}
				<div class="flex flex-wrap items-end gap-3 rounded-md border border-border p-3">
					<label class="block text-small">
						<span class="text-muted-foreground">{t('page.settings.remoteSource')}</span>
						<Select
							class="mt-1 w-56"
							value={source}
							options={sourceOptions}
							onchange={(value) => {
								source = value;
								resetResults();
							}}
						/>
					</label>
					<label class="block text-small">
						<span class="text-muted-foreground">{t('page.settings.remoteScope')}</span>
						<Select
							class="mt-1 w-40"
							value={scope}
							options={scopeOptions}
							onchange={(value) => {
								scope = value as DeployScope;
								resetResults();
							}}
						/>
					</label>
					<label class="block text-small">
						<span class="text-muted-foreground">{t('page.settings.remoteArtifactKind')}</span>
						<Select
							class="mt-1 w-40"
							value={artifactKind}
							options={artifactOptions}
							onchange={(value) => {
								artifactKind = value as 'release' | 'local';
								resetResults();
							}}
						/>
					</label>
					{#if artifactKind === 'local'}
						<label class="block text-small">
							<span class="text-muted-foreground">{t('page.settings.remoteArtifactPath')}</span>
							<input
								class="mt-1 w-64 rounded-md border border-input bg-card p-2 font-mono text-small"
								bind:value={artifactPath}
								placeholder="/path/to/wf-server"
							/>
						</label>
					{/if}
					<p class="w-full text-small text-muted-foreground">
						{t('page.settings.remoteSourceHint')}
						{#if selectedSource?.problem}· {selectedSource.problem}{/if}
					</p>
				</div>
			{:else if action === 'uninstall'}
				<p class="rounded-md border border-border p-3 text-small text-muted-foreground">
					{t('page.settings.remoteUninstallAll')}
				</p>
			{/if}

			<div class="flex items-center gap-2">
				<Button size="sm" disabled={busy} onclick={() => void runPlan()}>
					{t('page.settings.remotePlan')}
				</Button>
				<Button
					size="sm"
					variant="brand"
					disabled={busy || !plan || !!plan.blocked || plan.steps.length === 0}
					onclick={() => void runApply()}
				>
					{t('page.settings.remoteApply')}
				</Button>
			</div>

			{#if plan}
				<div class="space-y-2 rounded-md border border-border p-3">
					<p class="text-small text-muted-foreground">
						{t('page.settings.remoteInstallRoot')}: <span class="font-mono">{plan.installRoot}</span>
						· {t('page.settings.remoteServerPort')}: <span class="font-mono">{plan.serverPort}</span>
						· {t('page.settings.remoteScope')}: <span class="font-mono">{plan.scope}</span>
						{#if plan.source} · {t('page.settings.remoteSource')}: <span class="font-mono">{plan.source}</span>{/if}
					</p>
					{#if plan.artifact}
						<p class="text-small text-muted-foreground">
							{t('page.settings.remoteArtifactKind')}:
							<span class="font-mono">{plan.artifact.kind}</span>
							<span class="font-mono break-all">{plan.artifact.source}</span>
							<span class="font-mono">{plan.artifact.status}</span>
						</p>
					{/if}
					{#if plan.files.length > 0}
						<p class="text-small text-muted-foreground">
							{t('page.settings.remoteFilesToTransfer')}:
							{#each plan.files as file}
								<span class="ml-1 font-mono">{file.path} ({file.bytes}B, {file.mode})</span>
							{/each}
						</p>
					{/if}
					{#if plan.blocked}
						<p class="text-small text-destructive">
							{t('page.settings.remoteBlocked')}: {plan.blocked}
						</p>
					{/if}
					{#each plan.warnings as warning}
						<p class="text-small text-muted-foreground">! {warning}</p>
					{/each}
					<ol class="space-y-2">
						{#each plan.steps as step}
							<li class="text-small">
								<p class="font-medium">{step.description}</p>
								<pre class="mt-1 overflow-x-auto rounded bg-muted/40 p-2 font-mono text-micro">{step.command}</pre>
							</li>
						{/each}
					</ol>
				</div>
			{/if}

			{#if busy && deployJob}
				<div class="rounded-md border border-border p-3">
					<p class="text-small font-medium">
						{t('page.settings.remoteJobProgress', {
							step: deployJob.currentStep ?? '—',
							done: deployJob.steps.length,
							total: deployJob.steps.length + (deployJob.currentStep ? 1 : 0),
						})}
					</p>
					<p class="text-micro text-muted-foreground font-mono">{deployJob.jobId}</p>
				</div>
			{/if}

			{#if outcome}
				<div class="space-y-2 rounded-md border border-border p-3">
					<p class="text-small font-medium" class:text-destructive={!outcome.ok}>
						{outcome.ok ? t('page.settings.remoteOutcomeOk') : t('page.settings.remoteOutcomeFailed')}
						{#if outcome.version}· <span class="font-mono">{outcome.version}</span>{/if}
					</p>
					{#if outcome.error}
						<p class="text-small text-destructive">{outcome.error}</p>
					{/if}
					<ul class="space-y-1">
						{#each outcome.steps as step}
							<li class="text-small">
								<span class="font-mono">{step.ok ? 'ok' : 'fail'}</span>
								<span class="ml-2">{step.id}</span>
								{#if step.stderr}
									<pre class="mt-1 overflow-x-auto rounded bg-muted/40 p-2 font-mono text-micro">{step.stderr}</pre>
								{/if}
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
</Card>
