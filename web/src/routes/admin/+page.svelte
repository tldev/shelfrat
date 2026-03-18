<script lang="ts">
	import { onMount } from 'svelte';
	import { getLibraryInfo, getSettings, updateSettings, triggerScan, rebuildIndex, getProviders, updateProviders, testHardcoverKey, resetProvider, type ProviderInfo } from '$lib/api';
	import LockedField from '$lib/LockedField.svelte';

	let libraryInfo: any = $state(null);
	let settings: Record<string, string> = $state({});
	let envLocked: string[] = $state([]);
	let loading = $state(true);
	let scanning = $state(false);
	let reindexing = $state(false);
	let scanResult = $state('');
	let savingPath = $state(false);
	let pathMessage = $state('');
	let savingRetry = $state(false);
	let retryMessage = $state('');

	let providers: ProviderInfo[] = $state([]);
	let providerMessage = $state('');
	let apiKey = $state('');
	let testingKey = $state(false);
	let keyMessage = $state('');
	let resetting: string | null = $state(null);
	let dragIndex: number | null = $state(null);

	onMount(() => {
		loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const [info, settingsRes, providersRes] = await Promise.all([
				getLibraryInfo(),
				getSettings(),
				getProviders(),
			]);
			libraryInfo = info;
			settings = settingsRes.settings;
			envLocked = settingsRes.env_locked;
			providers = providersRes.providers;
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	async function handleSaveLibraryPath() {
		savingPath = true;
		pathMessage = '';
		try {
			const res = await updateSettings({ library_path: settings.library_path });
			pathMessage = 'library path updated';
			await loadData();
		} catch (err: any) {
			pathMessage = err.message || 'Failed to save';
		} finally {
			savingPath = false;
		}
	}

	async function handleSaveRetryHours() {
		savingRetry = true;
		retryMessage = '';
		try {
			await updateSettings({ metadata_retry_hours: settings.metadata_retry_hours });
			retryMessage = 'updated';
		} catch (err: any) {
			retryMessage = err.message || 'Failed to save';
		} finally {
			savingRetry = false;
		}
	}

	async function handleScan() {
		scanning = true;
		scanResult = '';
		try {
			const res = await triggerScan();
			scanResult = `scanned ${res.total_scanned}, imported ${res.imported}, updated ${res.updated}, queued ${res.metadata_queued} for metadata`;
		} catch (err: any) {
			scanResult = err.message || 'Scan failed';
		} finally {
			scanning = false;
		}
	}

	async function handleReindex() {
		reindexing = true;
		try {
			const res = await rebuildIndex();
			scanResult = `indexed ${res.indexed} books`;
		} catch (err: any) {
			scanResult = err.message || 'Reindex failed';
		} finally {
			reindexing = false;
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
			const providersRes = await getProviders();
			providers = providersRes.providers;
		} catch (err: any) {
			keyMessage = err.message || 'Failed to validate key';
		} finally {
			testingKey = false;
		}
	}

	async function saveProviders() {
		providerMessage = '';
		try {
			const enabled = providers.filter((p) => p.enabled).map((p) => p.name);
			await updateProviders(enabled);
		} catch (err: any) {
			providerMessage = err.message || 'Failed to save';
		}
	}

	function toggleProvider(name: string) {
		const idx = providers.findIndex((p) => p.name === name);
		if (idx === -1) return;
		if (name === 'hardcover' && !providers[idx].key_configured && !providers[idx].enabled) return;
		providers[idx] = { ...providers[idx], enabled: !providers[idx].enabled };
		saveProviders();
	}

	function moveUp(idx: number) {
		if (idx <= 0) return;
		const tmp = providers[idx];
		providers[idx] = providers[idx - 1];
		providers[idx - 1] = tmp;
		providers = [...providers];
		saveProviders();
	}

	function moveDown(idx: number) {
		if (idx >= providers.length - 1) return;
		const tmp = providers[idx];
		providers[idx] = providers[idx + 1];
		providers[idx + 1] = tmp;
		providers = [...providers];
		saveProviders();
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
		saveProviders();
	}

	async function handleReset(name: string) {
		resetting = name;
		providerMessage = '';
		try {
			const res = await resetProvider(name);
			providerMessage = `${name}: ${res.cleared} records cleared`;
		} catch (err: any) {
			providerMessage = err.message || 'Failed to reset';
		} finally {
			resetting = null;
		}
	}
</script>

{#if loading}
	<p class="status">loading...</p>
{:else}
	<div class="two-col">
		<div class="col-left">
			{#if libraryInfo}
				<section>
					<h2>library</h2>
					<div class="stats">
						<div class="stat">
							<span class="stat-value">{libraryInfo.available_books}</span>
							<span class="stat-label">books</span>
						</div>
						<div class="stat">
							<span class="stat-value">{libraryInfo.total_authors}</span>
							<span class="stat-label">authors</span>
						</div>
						{#if libraryInfo.missing_books > 0}
							<div class="stat">
								<span class="stat-value">{libraryInfo.missing_books}</span>
								<span class="stat-label">missing</span>
							</div>
						{/if}
					</div>

					<div class="library-path-field">
						<LockedField key="library_path" label="library path" placeholder="/path/to/books" bind:value={settings.library_path} {envLocked} />
						{#if !envLocked.includes('library_path')}
							<button class="secondary" onclick={handleSaveLibraryPath} disabled={savingPath}>
								{savingPath ? 'saving...' : 'save'}
							</button>
						{/if}
						{#if pathMessage}
							<p class="result">{pathMessage}</p>
						{/if}
					</div>

					<div class="library-path-field">
						<LockedField key="metadata_retry_hours" label="metadata retry (hours)" type="number" placeholder="24" hint="skip re-fetching metadata from providers within this window" bind:value={settings.metadata_retry_hours} {envLocked} />
						{#if !envLocked.includes('metadata_retry_hours')}
							<button class="secondary" onclick={handleSaveRetryHours} disabled={savingRetry}>
								{savingRetry ? 'saving...' : 'save'}
							</button>
						{/if}
						{#if retryMessage}
							<p class="result">{retryMessage}</p>
						{/if}
					</div>

					{#if libraryInfo.format_breakdown.length > 0}
						<div class="formats">
							{#each libraryInfo.format_breakdown as f}
								<span class="badge">{f.format} ({f.count})</span>
							{/each}
						</div>
					{/if}

					<div class="actions">
						<button onclick={handleScan} disabled={scanning}>
							{scanning ? 'scanning...' : 'full scan'}
						</button>
						<button class="secondary" onclick={handleReindex} disabled={reindexing}>
							{reindexing ? 'indexing...' : 'rebuild index'}
						</button>
					</div>
					{#if scanResult}
						<p class="result">{scanResult}</p>
					{/if}
				</section>
			{/if}
		</div>

		<div class="col-right">
			<section>
				<h2>metadata providers</h2>
				<span class="hint">enabled providers are tried in order during enrichment</span>
				<div class="provider-list">
					{#each providers as provider, idx}
						<div
							class="provider-item"
							class:disabled={!provider.enabled}
							class:dragging={dragIndex === idx}
							draggable="true"
							ondragstart={() => handleDragStart(idx)}
							ondragover={(e) => handleDragOver(e, idx)}
							ondragend={handleDragEnd}
							role="listitem"
						>
							<div class="provider-row">
								<span class="drag-handle" title="drag to reorder">::</span>
								<span class="provider-name">{provider.name}</span>
								{#if provider.name === 'hardcover' && !provider.key_configured}
									<span class="hint">set key</span>
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
							<label class="toggle">
								<input
									type="checkbox"
									checked={provider.enabled}
									onchange={() => toggleProvider(provider.name)}
									disabled={provider.name === 'hardcover' && !provider.key_configured}
								/>
								<span class="toggle-box">{provider.enabled ? 'x' : '\u00a0'}</span>
							</label>
						</div>
					{/each}
				</div>
				{#if providerMessage}
					<p class="result">{providerMessage}</p>
				{/if}
			</section>

			<section>
				<h2>hardcover API key</h2>
				{#if providers.find((p) => p.name === 'hardcover')?.key_configured}
					<span class="badge configured">configured</span>
				{/if}
				{#if envLocked.includes('hardcover_api_key')}
					<span class="hint env-hint">set by environment variable SHELFRAT_HARDCOVER_API_KEY</span>
				{:else}
					<div class="key-row">
						<input type="password" bind:value={apiKey} placeholder="Bearer eyJhb..." />
						<button class="secondary" onclick={handleTestKey} disabled={testingKey || !apiKey.trim()}>
							{testingKey ? 'testing...' : 'test & save'}
						</button>
					</div>
					<span class="hint">validated before saving — <a href="https://hardcover.app/account/api" target="_blank" rel="noopener">get your API key</a></span>
				{/if}
				{#if keyMessage}
					<p class="result">{keyMessage}</p>
				{/if}
			</section>
		</div>
	</div>
{/if}

<style>
	.two-col {
		display: flex;
		gap: 2.5rem;
		align-items: flex-start;
	}

	.col-left {
		flex: 1;
	}

	.col-right {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	section {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.stats {
		display: flex;
		gap: 2rem;
	}

	.stat {
		display: flex;
		flex-direction: column;
	}

	.stat-value {
		font-size: 1.5rem;
		font-weight: 500;
	}

	.stat-label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
	}

	.formats {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
	}

	.library-path-field {
		max-width: var(--max-w-narrow);
	}

	.provider-list {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.provider-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		cursor: grab;
	}

	.provider-item.disabled {
		opacity: 0.5;
	}

	.provider-item.dragging {
		opacity: 0.4;
	}

	.provider-row {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--border);
	}

	.drag-handle {
		cursor: grab;
		color: var(--fg-muted);
		font-weight: bold;
		user-select: none;
	}

	.toggle {
		cursor: pointer;
		display: flex;
		align-items: center;
		flex-shrink: 0;
	}

	.toggle input {
		position: absolute;
		opacity: 0;
		width: 0;
		height: 0;
	}

	.toggle-box {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.25rem;
		height: 1.25rem;
		border: 1px solid var(--fg);
		background: transparent;
		font-size: 0.75rem;
		line-height: 1;
		color: var(--fg);
	}

	.toggle input:checked + .toggle-box {
		border-color: var(--fg);
	}

	.toggle input:disabled + .toggle-box {
		opacity: 0.3;
		cursor: not-allowed;
	}

	.provider-name {
		flex: 1;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
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

	.configured {
		background: var(--bg-offset, rgba(128, 128, 128, 0.08));
		font-size: 0.7rem;
		align-self: flex-start;
	}

	.key-row {
		display: flex;
		gap: 0.5rem;
	}

	.key-row input {
		flex: 1;
	}

	@media (max-width: 640px) {
		.two-col {
			flex-direction: column;
			gap: 1.5rem;
		}

		.stats {
			display: grid;
			grid-template-columns: 1fr 1fr;
			gap: 1rem;
		}

		.actions {
			flex-direction: column;
		}

		.actions button {
			width: 100%;
		}

		.library-path-field {
			max-width: none;
		}
	}
</style>
