<script lang="ts">
	import { coverUrl, type Book } from '$lib/api';

	let { book, onselect }: { book: Book; onselect: () => void } = $props();
	let imgError = $state(false);

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
		return `${(bytes / 1048576).toFixed(1)} MB`;
	}
</script>

<button class="card" onclick={onselect}>
	<div class="cover">
		{#if !imgError}
			<img
				src={coverUrl(book.id)}
				alt={book.title || 'Book cover'}
				onerror={() => (imgError = true)}
			/>
		{:else}
			<div class="placeholder">
				<span>{(book.title || '?')[0]}</span>
			</div>
		{/if}
	</div>
	<div class="info">
		<p class="title">{book.title || 'Untitled'}</p>
		{#if book.authors.length > 0}
			<p class="author">{book.authors.join(', ')}</p>
		{/if}
		<div class="meta">
			<span class="badge">{book.file_format}</span>
		</div>
	</div>
</button>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		text-align: left;
		background: transparent;
		border: none;
		color: var(--fg);
		padding: 0;
		cursor: pointer;
	}

	.card:hover .title {
		color: var(--accent);
	}

	.cover {
		aspect-ratio: 2 / 3;
		overflow: hidden;
		background: var(--surface);
		border: 1px solid var(--border);
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 2rem;
		color: var(--fg-muted);
		font-weight: 500;
	}

	.info {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.title {
		font-size: 0.8rem;
		font-weight: 500;
		line-height: 1.3;
		overflow: hidden;
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		transition: color 0.15s;
	}

	.author {
		font-size: 0.7rem;
		color: var(--fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta {
		margin-top: 0.15rem;
	}
</style>
