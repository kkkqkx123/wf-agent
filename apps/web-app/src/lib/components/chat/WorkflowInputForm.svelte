<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import IssueList from '#lib/components/domain/IssueList.svelte';
	import JsonEditor from '#lib/ui/components/JsonEditor.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import { t } from '#lib/i18n';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';
	import {
		parseWorkflowInputText,
		validateWorkflowInput,
	} from '#lib/services/run';
	import {
		jsonErrorLine,
		type TemplateIssue,
	} from '#lib/services/templates';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const variables = $derived(controller.workflowVariables);
	const loading = $derived(controller.workflowVariablesLoading);
	const strict = $derived(controller.workflowStrict);

	let inputTab = $state<'form' | 'json'>('form');
	let jsonText = $state('');
	let syntaxError = $state<string | null>(null);
	let jsonEditor = $state<{ scrollToLine: (line: number) => void } | null>(null);

	const errorLine = $derived(
		syntaxError ? jsonErrorLine(jsonText, syntaxError) : null,
	);

	/** Semantic issues of the applied inputs, shown while the JSON tab is
	 * active. The form tab guides per field, so the list lives here only. */
	const semanticIssues = $derived.by<TemplateIssue[]>(() => {
		if (inputTab !== 'json' || syntaxError) return [];
		return validateWorkflowInput(
			{ ...(controller.workflowInputs as Record<string, unknown>) },
			variables,
			strict,
		).map((message) => ({ source: 'local' as const, field: null, message, nodeId: null }));
	});

	const jsonCount = $derived(
		(syntaxError ? 1 : 0) + (inputTab === 'json' ? semanticIssues.length : 0),
	);

	/** Enter the JSON tab from the applied values. Re-serializing keeps one
	 * source of truth: unparsable text is never applied, so returning always
	 * shows what will actually run. */
	function enterJson(): void {
		try {
			jsonText = JSON.stringify(controller.workflowInputs ?? {}, null, 2);
		} catch {
			jsonText = '{}';
		}
		syntaxError = null;
		inputTab = 'json';
	}

	function enterForm(): void {
		syntaxError = null;
		controller.workflowJsonError = null;
		inputTab = 'form';
	}

	function fieldKind(type: string | null | undefined): string {
		if (type === 'number' || type === 'boolean' || type === 'string') return type;
		return 'json';
	}

	function readValue(name: string): string {
		const value = (controller.workflowInputs as Record<string, unknown>)[name];
		if (value === undefined || value === null) return '';
		if (typeof value === 'string') return value;
		try {
			return JSON.stringify(value);
		} catch {
			return String(value);
		}
	}

	function writeValue(name: string, raw: string, type: string | null | undefined): void {
		// Form edits supersede any pending JSON text; the next visit to the
		// JSON tab re-serializes from the applied values.
		syntaxError = null;
		controller.workflowJsonError = null;
		const trimmed = raw.trim();
		if (trimmed === '') {
			const next = { ...(controller.workflowInputs as Record<string, unknown>) };
			delete next[name];
			controller.workflowInputs = next;
			return;
		}
		let parsed: unknown = raw;
		if (type === 'number') {
			const num = Number(trimmed);
			parsed = Number.isFinite(num) ? num : raw;
		} else if (type === 'boolean') {
			if (trimmed === 'true') parsed = true;
			else if (trimmed === 'false') parsed = false;
			else parsed = raw;
		} else if (type === 'object' || type === 'array') {
			try {
				parsed = JSON.parse(trimmed) as unknown;
			} catch {
				parsed = raw;
			}
		}
		controller.workflowInputs = {
			...(controller.workflowInputs as Record<string, unknown>),
			[name]: parsed,
		};
	}

	// Parse JSON tab edits into the shared inputs. Valid objects (blank
	// clears to no input) apply live; syntax failures only set the error so
	// the applied values stay runnable. Never writes back to the text, so
	// typing never fights the editor.
	$effect(() => {
		if (inputTab !== 'json') return;
		const text = jsonText;
		let parsed: Record<string, unknown>;
		try {
			parsed = (parseWorkflowInputText(text) ?? {}) as Record<string, unknown>;
		} catch (e) {
			syntaxError = e instanceof Error ? e.message : 'Workflow input must be valid JSON.';
			controller.workflowJsonError = syntaxError;
			return;
		}
		syntaxError = null;
		controller.workflowJsonError = null;
		controller.workflowInputs = parsed;
	});
</script>

{#if controller.executionType === 'workflow'}
	{#if loading}
		<p class="text-xs text-muted-foreground">Loading workflow variables…</p>
	{:else}
		<Segmented
			items={[
				{ id: 'form', label: t('page.resources.toolRun.form') },
				{
					id: 'json',
					label: t('page.resources.toolRun.json'),
					count: jsonCount > 0 ? jsonCount : undefined,
				},
			]}
			value={inputTab}
			size="sm"
			onchange={(id) => {
				if (id === 'json') enterJson();
				else enterForm();
			}}
		/>
		{#if inputTab === 'form'}
			{#if variables.length > 0}
				<div class="mt-2 grid gap-2 rounded-lg border border-border p-2.5">
					{#each variables as variable (variable.name)}
						{@const kind = fieldKind(variable.type)}
						{@const required = variable.required === true}
						<label class="grid gap-1 text-xs">
							<span class="font-medium">
								{variable.name}
								{#if required}<span class="text-destructive"> *</span>{/if}
								{#if variable.type}<span class="text-muted-foreground"> ({variable.type})</span>{/if}
							</span>
							{#if kind === 'boolean'}
								<select
									value={readValue(variable.name)}
									onchange={(event) =>
										writeValue(variable.name, (event.target as HTMLSelectElement).value, variable.type)}
								>
									<option value="">—</option>
									<option value="true">true</option>
									<option value="false">false</option>
								</select>
							{:else if kind === 'number'}
								<input
									type="number"
									value={readValue(variable.name)}
									placeholder={variable.value !== undefined && variable.value !== null
										? String(variable.value)
										: ''}
									oninput={(event) =>
										writeValue(variable.name, (event.target as HTMLInputElement).value, variable.type)}
								/>
							{:else}
								<input
									type="text"
									value={readValue(variable.name)}
									placeholder={variable.value !== undefined && variable.value !== null
										? String(variable.value)
										: kind === 'json'
											? '{"key": "value"}'
											: ''}
									oninput={(event) =>
										writeValue(variable.name, (event.target as HTMLInputElement).value, variable.type)}
								/>
							{/if}
						</label>
					{/each}
				</div>
			{:else}
				<p class="mt-2 text-xs text-muted-foreground">
					No declared variables; add entries in the JSON tab.
				</p>
			{/if}
		{:else}
			<div class="mt-2">
				<JsonEditor
					bind:this={jsonEditor}
					bind:value={jsonText}
					errorLine={errorLine}
					placeholder={'{\n  "key": "value"\n}'}
					label="Workflow input JSON"
					minHeight="10rem"
				/>
			</div>
			{#if syntaxError}
				<div
					class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
				>
					<p class="text-caption text-destructive">
						{t('page.resources.toolRun.syntaxError', {
							line: errorLine ? ` (${errorLine})` : '',
							error: syntaxError,
						})}
					</p>
					{#if errorLine}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => jsonEditor?.scrollToLine(errorLine ?? 1)}
						>
							{t('page.resources.toolRun.goToLine')}
						</Button>
					{/if}
				</div>
			{/if}
			{#if semanticIssues.length > 0}
				<div class="mt-2">
					<IssueList issues={semanticIssues} />
				</div>
			{/if}
		{/if}
	{/if}
{/if}
