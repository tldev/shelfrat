<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { getAuth } from '$lib/auth.svelte';
	import { getLibraryInfo, getSettings, updateSettings, triggerScan, rebuildIndex } from '$lib/api';

	const auth = getAuth();

	let libraryInfo: any = $state(null);
	let settings: Record<string, string> = $state({});
	let loading = $state(true);
	let scanning = $state(false);
	let reindexing = $state(false);
	let scanResult = $state('');
	let savingPath = $state(false);
	let pathMessage = $state('');
	let savingRetry = $state(false);
	let retryMessage = $state('');

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
			const [info, settingsRes] = await Promise.all([getLibraryInfo(), getSettings()]);
			libraryInfo = info;
			settings = settingsRes.settings;
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
</script>

<div class="admin">
	<h1>admin</h1>

	<nav class="admin-nav">
		<a href="/admin" class="active">settings</a>
		<a href="/admin/users">users</a>
		<a href="/admin/auth">auth</a>
		<a href="/admin/smtp">smtp</a>
		<a href="/admin/jobs">jobs</a>
		<a href="/admin/audit">audit log</a>
	</nav>

	{#if loading}
		<p class="status">loading...</p>
	{:else}
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

				<div class="field library-path-field">
					<label for="library_path">library path</label>
					<div class="path-row">
						<input id="library_path" type="text" bind:value={settings.library_path} placeholder="/path/to/books" />
						<button class="secondary" onclick={handleSaveLibraryPath} disabled={savingPath}>
							{savingPath ? 'saving...' : 'save'}
						</button>
					</div>
					{#if pathMessage}
						<p class="result">{pathMessage}</p>
					{/if}
				</div>

				<div class="field library-path-field">
					<label for="metadata_retry_hours">metadata retry (hours)</label>
					<div class="path-row">
						<input id="metadata_retry_hours" type="number" min="0" bind:value={settings.metadata_retry_hours} placeholder="24" />
						<button class="secondary" onclick={handleSaveRetryHours} disabled={savingRetry}>
							{savingRetry ? 'saving...' : 'save'}
						</button>
					</div>
					<span class="hint">skip re-fetching metadata from providers within this window</span>
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

	.result {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.library-path-field {
		max-width: var(--max-w-narrow);
	}

	.path-row {
		display: flex;
		gap: 0.5rem;
	}

	.path-row input {
		flex: 1;
	}

	.field {
		display: flex;
		flex-direction: column;
	}

	.hint {
		font-size: 0.7rem;
		color: var(--fg-muted);
	}

	.status {
		color: var(--fg-muted);
		font-size: 0.85rem;
	}

	@media (max-width: 640px) {
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
