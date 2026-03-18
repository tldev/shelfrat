<script lang="ts">
	let {
		key,
		label,
		type = 'text',
		placeholder = '',
		hint = '',
		value = $bindable(),
		envLocked = [] as string[],
		options = [] as { value: string; label: string }[],
	}: {
		key: string;
		label: string;
		type?: string;
		placeholder?: string;
		hint?: string;
		value?: string;
		envLocked: string[];
		options?: { value: string; label: string }[];
	} = $props();

	const locked = $derived(envLocked.includes(key));
	const envName = $derived('SHELFRAT_' + key.toUpperCase().replace(/:/g, '_'));

	let copied = $state(false);

	function copyEnvName() {
		navigator.clipboard.writeText(envName);
		copied = true;
		setTimeout(() => (copied = false), 1500);
	}
</script>

<div class="field">
	<span class="label-wrap">
		<label for={key}>{label}</label>
		<button type="button" class="env-icon" onclick={copyEnvName} title="click to copy {envName}">
			{copied ? 'copied' : '$'}
		</button>
		{#if !copied}
			<span class="env-name">{envName}</span>
		{/if}
	</span>
	{#if options.length > 0}
		<select id={key} bind:value disabled={locked}>
			{#each options as opt}
				<option value={opt.value}>{opt.label}</option>
			{/each}
		</select>
	{:else}
		<input id={key} {type} bind:value {placeholder} disabled={locked} />
	{/if}
	{#if locked}
		<span class="hint env-hint">set by environment variable {envName}</span>
	{:else if hint}
		<span class="hint">{hint}</span>
	{/if}
</div>

<style>
	.label-wrap {
		position: relative;
		display: inline-flex;
		align-items: baseline;
		gap: 0.35rem;
		align-self: flex-start;
	}

	.label-wrap label {
		margin-bottom: 0;
	}

	.env-icon {
		all: unset;
		cursor: pointer;
		font-size: 0.65rem;
		color: var(--fg-muted);
		white-space: nowrap;
		letter-spacing: 0;
		text-transform: none;
		opacity: 0.5;
	}

	.env-icon:hover {
		opacity: 1;
		color: var(--fg);
	}

	.env-name {
		display: none;
		font-size: 0.65rem;
		color: var(--fg-muted);
		white-space: nowrap;
		letter-spacing: 0;
		text-transform: none;
	}

	.env-icon:hover + .env-name {
		display: inline;
	}
</style>
