<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { getAuth } from '$lib/auth.svelte';
	import { getProviders, updateProviders, testHardcoverKey, resetProvider, type ProviderInfo } from '$lib/api';

	const auth = getAuth();

	let providers: ProviderInfo[] = $state([]);
	let loading = $state(true);
	let saving = $state(false);
	let message = $state('');

	let apiKey = $state('');
	let testingKey = $state(false);
	let keyMessage = $state('');

	let resetting: string | null = $state(null);

	let dragIndex: number | null = $state(null);

	onMount(async () => {
		if (!auth.isAdmin) {
			goto('/');
			return;
		}
		await loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const res = await getProviders();
			providers = res.providers;
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	async function handleTestKey() {
		if (!apiKey.trim()) return;
		testingKey = true;
		keyMessage = '';
		try {
			const res = await testHardcoverKey(apiKey.trim());
			keyMessage = res.message;
			apiKey = '';
			await loadData();
		} catch (err: any) {
			keyMessage = err.message || 'Failed to validate key';
		} finally {
			testingKey = false;
		}
	}

	function toggleProvider(name: string) {
		const idx = providers.findIndex((p) => p.name === name);
		if (idx === -1) return;
		if (name === 'hardcover' && !providers[idx].key_configured && !providers[idx].enabled) return;
		providers[idx] = { ...providers[idx], enabled: !providers[idx].enabled };
	}

	function moveUp(idx: number) {
		if (idx <= 0) return;
		const tmp = providers[idx];
		providers[idx] = providers[idx - 1];
		providers[idx - 1] = tmp;
		providers = [...providers];
	}

	function moveDown(idx: number) {
		if (idx >= providers.length - 1) return;
		const tmp = providers[idx];
		providers[idx] = providers[idx + 1];
		providers[idx + 1] = tmp;
		providers = [...providers];
	}

	function handleDragStart(idx: number) {
		dragIndex = idx;
	}

	function handleDragOver(e: DragEvent, idx: number) {
		e.preventDefault();
		if (dragIndex === null || dragIndex === idx) return;
		const item = providers[dragIndex];
		providers.splice(dragIndex, 1);
		providers.splice(idx, 0, item);
		dragIndex = idx;
		providers = [...providers];
	}

	function handleDragEnd() {
		dragIndex = null;
	}

	async function handleReset(name: string) {
		resetting = name;
		message = '';
		try {
			const res = await resetProvider(name);
			message = `${name}: ${res.cleared} records cleared`;
		} catch (err: any) {
			message = err.message || 'Failed to reset';
		} finally {
			resetting = null;
		}
	}

	async function handleSave() {
		saving = true;
		message = '';
		try {
			const enabled = providers.filter((p) => p.enabled).map((p) => p.name);
			await updateProviders(enabled);
			message = 'providers updated';
			await loadData();
		} catch (err: any) {
			message = err.message || 'Failed to save';
		} finally {
			saving = false;
		}
	}
</script>

<div class="admin">
	<h1>admin</h1>

	<nav class="admin-nav">
		<a href="/admin">settings</a>
		<a href="/admin/users">users</a>
		<a href="/admin/auth">auth</a>
		<a href="/admin/providers" class="active">providers</a>
		<a href="/admin/smtp">smtp</a>
		<a href="/admin/jobs">jobs</a>
		<a href="/admin/audit">audit log</a>
	</nav>

	{#if loading}
		<p class="status">loading...</p>
	{:else}
		<section>
			<h2>hardcover API key</h2>
			<div class="key-section">
				{#if providers.find((p) => p.name === 'hardcover')?.key_configured}
					<span class="badge configured">configured</span>
				{/if}
				<div class="key-row">
					<input type="password" bind:value={apiKey} placeholder="hc_live_..." />
					<button class="secondary" onclick={handleTestKey} disabled={testingKey || !apiKey.trim()}>
						{testingKey ? 'testing...' : 'test & save'}
					</button>
				</div>
				<span class="hint">key is validated against Hardcover API before saving</span>
				{#if keyMessage}
					<p class="result">{keyMessage}</p>
				{/if}
			</div>
		</section>

		<section>
			<h2>provider order</h2>
			<span class="hint">drag to reorder. enabled providers are tried in order during metadata enrichment.</span>
			<div class="provider-list">
				{#each providers as provider, idx}
					<div
						class="provider-row"
						class:disabled={!provider.enabled}
						class:dragging={dragIndex === idx}
						draggable="true"
						ondragstart={() => handleDragStart(idx)}
						ondragover={(e) => handleDragOver(e, idx)}
						ondragend={handleDragEnd}
						role="listitem"
					>
						<span class="drag-handle" title="drag to reorder">::</span>
						<label class="provider-label">
							<input
								type="checkbox"
								checked={provider.enabled}
								onchange={() => toggleProvider(provider.name)}
								disabled={provider.name === 'hardcover' && !provider.key_configured}
							/>
							{provider.name}
						</label>
						{#if provider.name === 'hardcover' && !provider.key_configured}
							<span class="hint">configure API key to enable</span>
						{/if}
						<button
							class="secondary small"
							onclick={() => handleReset(provider.name)}
							disabled={resetting === provider.name}
							title="clear attempt history so books are re-queried"
						>
							{resetting === provider.name ? '...' : 'reset'}
						</button>
						<div class="move-buttons">
							<button class="secondary small" onclick={() => moveUp(idx)} disabled={idx === 0} title="move up">^</button>
							<button class="secondary small" onclick={() => moveDown(idx)} disabled={idx === providers.length - 1} title="move down">v</button>
						</div>
					</div>
				{/each}
			</div>
			{#if message}
				<p class="result">{message}</p>
			{/if}
			<button onclick={handleSave} disabled={saving}>
				{saving ? 'saving...' : 'save'}
			</button>
		</section>
	{/if}
</div>

<style>
	.admin {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.admin-nav {
		display: flex;
		gap: 1.5rem;
		border-bottom: 1px solid var(--border);
		padding-bottom: 0.75rem;
	}

	.admin-nav a {
		font-size: 0.8rem;
		color: var(--fg-muted);
		text-decoration: none;
	}

	.admin-nav a:hover,
	.admin-nav a.active {
		color: var(--fg);
	}

	section {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.key-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: var(--max-w-narrow);
	}

	.key-row {
		display: flex;
		gap: 0.5rem;
	}

	.key-row input {
		flex: 1;
	}

	.configured {
		background: var(--bg-offset, rgba(128, 128, 128, 0.08));
		font-size: 0.7rem;
		align-self: flex-start;
	}

	.provider-list {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		max-width: var(--max-w-narrow);
	}

	.provider-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem;
		border: 1px solid var(--border);
		cursor: grab;
	}

	.provider-row.disabled {
		opacity: 0.5;
	}

	.provider-row.dragging {
		opacity: 0.4;
	}

	.drag-handle {
		cursor: grab;
		color: var(--fg-muted);
		font-weight: bold;
		user-select: none;
	}

	.provider-label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		cursor: pointer;
	}

	.move-buttons {
		display: flex;
		gap: 0.25rem;
	}

	.small {
		font-size: 0.7rem;
		padding: 0.15rem 0.4rem;
		min-width: 1.5rem;
	}

	.hint {
		font-size: 0.7rem;
		color: var(--fg-muted);
	}

	.result {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.status {
		color: var(--fg-muted);
		font-size: 0.85rem;
	}

	@media (max-width: 640px) {
		.key-section {
			max-width: none;
		}

		.provider-list {
			max-width: none;
		}
	}
</style>
