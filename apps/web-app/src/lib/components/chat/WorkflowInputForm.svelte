<script lang="ts">
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const variables = $derived(controller.workflowVariables);
	const loading = $derived(controller.workflowVariablesLoading);

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
</script>

{#if controller.executionType === 'workflow'}
	{#if loading}
		<p class="text-xs text-muted-foreground">Loading workflow variables…</p>
	{:else if variables.length > 0}
		<div class="grid gap-2 rounded-lg border border-border p-2.5">
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
	{/if}
{/if}
