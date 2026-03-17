<script lang="ts">
	import { onMount } from 'svelte';
	import { getBook, coverUrl, downloadUrl, sendToKindle, type BookDetail as BookDetailType } from '$lib/api';
	import { getAuth } from '$lib/auth.svelte';

	let { bookId, onclose }: { bookId: number; onclose: () => void } = $props();

	let detail: BookDetailType | null = $state(null);
	let loading = $state(true);
	let error = $state('');
	let sendingKindle = $state(false);
	let kindleMsg = $state('');
	let imgError = $state(false);

	const auth = getAuth();

	onMount(() => {
		// Prevent body scroll while modal is open
		document.body.style.overflow = 'hidden';

		getBook(bookId)
			.then((res) => (detail = res))
			.catch((err: any) => (error = err.message || 'Failed to load book'))
			.finally(() => (loading = false));

		return () => {
			document.body.style.overflow = '';
		};
	});

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onclose();
	}

	function handleOverlayClick(e: MouseEvent) {
		if ((e.target as HTMLElement).classList.contains('overlay')) {
			onclose();
		}
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
		return `${(bytes / 1048576).toFixed(1)} MB`;
	}

	async function handleSendKindle() {
		sendingKindle = true;
		kindleMsg = '';
		try {
			const res = await sendToKindle(bookId);
			kindleMsg = `sent to ${res.to}`;
		} catch (err: any) {
			kindleMsg = err.message || 'Failed to send';
		} finally {
			sendingKindle = false;
		}
	}

	async function download() {
		const token = localStorage.getItem('token');
		const res = await fetch(downloadUrl(bookId), {
			headers: token ? { Authorization: `Bearer ${token}` } : {}
		});
		if (!res.ok) {
			const body = await res.json().catch(() => ({ error: res.statusText }));
			alert(body.error || 'Download failed');
			return;
		}
		const blob = await res.blob();
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		const disposition = res.headers.get('content-disposition') || '';
		const match = disposition.match(/filename="([^"]+)"/);
		if (match) a.download = match[1];
		a.click();
		URL.revokeObjectURL(url);
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={handleOverlayClick}>
	<div class="panel">
		<button class="close" onclick={onclose}>&times;</button>

		{#if loading}
			<div class="content">
				<div class="cover">
					<div class="cover-skel skeleton"></div>
				</div>
				<div class="details">
					<div class="skeleton" style="height: 1.2rem; width: 70%;"></div>
					<div class="skeleton" style="height: 0.85rem; width: 40%;"></div>
					<div class="skeleton" style="height: 4rem; width: 100%; margin-top: 0.5rem;"></div>
					<div class="meta-grid" style="margin-top: 0.5rem;">
						<div class="meta-item"><div class="skeleton" style="height: 0.65rem; width: 3rem;"></div><div class="skeleton" style="height: 0.8rem; width: 5rem; margin-top: 0.2rem;"></div></div>
						<div class="meta-item"><div class="skeleton" style="height: 0.65rem; width: 3rem;"></div><div class="skeleton" style="height: 0.8rem; width: 4rem; margin-top: 0.2rem;"></div></div>
					</div>
				</div>
			</div>
		{:else if error}
			<p class="error">{error}</p>
		{:else if detail}
			<div class="content">
				<div class="cover">
					{#if !imgError}
						<img
							src={coverUrl(bookId)}
							alt={detail.metadata?.title || 'Cover'}
							onerror={() => (imgError = true)}
						/>
					{:else}
						<div class="placeholder">
							<span>{(detail.metadata?.title || '?')[0]}</span>
						</div>
					{/if}
				</div>

				<div class="details">
					<h2>{detail.metadata?.title || 'Untitled'}</h2>
					{#if detail.metadata?.subtitle}
						<p class="subtitle">{detail.metadata.subtitle}</p>
					{/if}
					{#if detail.authors.length > 0}
						<p class="authors">{detail.authors.join(', ')}</p>
					{/if}

					{#if detail.metadata?.description}
						<p class="description">{detail.metadata.description}</p>
					{/if}

					<div class="meta-grid">
						{#if detail.metadata?.publisher}
							<div class="meta-item">
								<span class="meta-label">publisher</span>
								<span>{detail.metadata.publisher}</span>
							</div>
						{/if}
						{#if detail.metadata?.published_date}
							<div class="meta-item">
								<span class="meta-label">published</span>
								<span>{detail.metadata.published_date}</span>
							</div>
						{/if}
						{#if detail.metadata?.page_count}
							<div class="meta-item">
								<span class="meta-label">pages</span>
								<span>{detail.metadata.page_count}</span>
							</div>
						{/if}
						{#if detail.metadata?.language}
							<div class="meta-item">
								<span class="meta-label">language</span>
								<span>{detail.metadata.language}</span>
							</div>
						{/if}
						{#if detail.metadata?.isbn_13 || detail.metadata?.isbn_10}
							<div class="meta-item">
								<span class="meta-label">isbn</span>
								<span>{detail.metadata.isbn_13 || detail.metadata.isbn_10}</span>
							</div>
						{/if}
						{#if detail.metadata?.series_name}
							<div class="meta-item">
								<span class="meta-label">series</span>
								<span>
									{detail.metadata.series_name}
									{#if detail.metadata.series_number}
										#{detail.metadata.series_number}
									{/if}
								</span>
							</div>
						{/if}
						<div class="meta-item">
							<span class="meta-label">format</span>
							<span>{detail.book.file_format.toUpperCase()} &middot; {formatSize(detail.book.file_size_bytes)}</span>
						</div>
					</div>

					{#if detail.tags.length > 0}
						<div class="tags">
							{#each detail.tags as tag}
								<span class="badge">{tag}</span>
							{/each}
						</div>
					{/if}

					<div class="actions">
						<button onclick={download}>download</button>
						<button class="secondary" onclick={handleSendKindle} disabled={sendingKindle}>
							{sendingKindle ? 'sending...' : 'send to kindle'}
						</button>
					</div>
					{#if kindleMsg}
						<p class="kindle-msg">{kindleMsg}</p>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
		padding: 2rem;
	}

	.panel {
		background: var(--bg);
		border: 1px solid var(--border);
		max-width: 48rem;
		width: 100%;
		max-height: 85vh;
		overflow-y: auto;
		padding: 2rem;
		position: relative;
	}

	.close {
		position: absolute;
		top: 0.75rem;
		right: 0.75rem;
		background: transparent;
		border: none;
		font-size: 1.5rem;
		color: var(--fg-muted);
		padding: 0.25rem 0.5rem;
		line-height: 1;
	}

	.close:hover {
		color: var(--fg);
	}

	.content {
		display: flex;
		gap: 2rem;
	}

	.cover {
		flex-shrink: 0;
		width: 200px;
	}

	.cover img {
		width: 100%;
		border: 1px solid var(--border);
	}

	.cover-skel {
		width: 100%;
		aspect-ratio: 2 / 3;
	}

	.placeholder {
		width: 100%;
		aspect-ratio: 2 / 3;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 3rem;
		color: var(--fg-muted);
		background: var(--surface);
		border: 1px solid var(--border);
	}

	.details {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.subtitle {
		color: var(--fg-muted);
		font-size: 0.9rem;
	}

	.authors {
		color: var(--accent);
		font-size: 0.85rem;
	}

	.description {
		font-size: 0.8rem;
		line-height: 1.6;
		color: var(--fg);
		max-height: 12rem;
		overflow-y: auto;
	}

	.meta-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.5rem;
	}

	.meta-item {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}

	.meta-label {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
	}

	.meta-item span:last-child {
		font-size: 0.8rem;
	}

	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}

	.kindle-msg {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	@media (max-width: 640px) {
		.overlay {
			padding: 0;
			align-items: stretch;
		}

		.panel {
			max-height: 100vh;
			height: 100vh;
			border: none;
			padding: 1.25rem 1rem;
			padding-top: 2.5rem;
		}

		.content {
			flex-direction: column;
		}

		.cover {
			width: 100px;
		}

		.meta-grid {
			grid-template-columns: 1fr;
		}

		.description {
			max-height: 8rem;
		}

		.actions {
			flex-direction: column;
		}

		.actions button {
			width: 100%;
		}
	}
</style>
