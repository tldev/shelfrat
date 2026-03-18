<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listBooks,
		searchBooks,
		listAuthors,
		listTags,
		listFormats,
		type Book,
		type FilterItem
	} from '$lib/api';
	import BookCard from '$lib/BookCard.svelte';
	import BookCardSkeleton from '$lib/BookCardSkeleton.svelte';
	import BookDetail from '$lib/BookDetail.svelte';

	let books: Book[] = $state([]);
	let total = $state(0);
	let query = $state('');
	let sort = $state('added');
	let loading = $state(true);
	let loadingMore = $state(false);
	let searchTimeout: ReturnType<typeof setTimeout>;
	let selectedBookId: number | null = $state(null);
	let offset = $state(0);
	const limit = 96;

	// Filters
	let authors: FilterItem[] = $state([]);
	let tags: FilterItem[] = $state([]);
	let formats: FilterItem[] = $state([]);
	let selectedAuthor = $state('');
	let selectedTag = $state('');
	let selectedFormat = $state('');
	let filtersOpen = $state(false);

	let hasActiveFilter = $derived(!!selectedAuthor || !!selectedTag || !!selectedFormat);
	let hasMore = $derived(!query.trim() && books.length < total);

	let sentinel: HTMLDivElement | undefined = $state();
	let observer: IntersectionObserver | undefined = $state();

	onMount(() => {
		loadBooks();
		// Load filter options in background
		Promise.all([listAuthors(), listTags(), listFormats()])
			.then(([a, t, f]) => {
				authors = a.authors;
				tags = t.tags;
				formats = f.formats;
			})
			.catch(() => {});

		observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting && hasMore && !loading && !loadingMore) {
					loadMore();
				}
			},
			{ rootMargin: '400px' }
		);

		return () => observer?.disconnect();
	});

	$effect(() => {
		if (sentinel && observer) {
			observer.disconnect();
			observer.observe(sentinel);
		}
	});

	async function loadBooks() {
		loading = true;
		offset = 0;
		try {
			if (query.trim()) {
				const res = await searchBooks(query, limit);
				books = res.books;
				total = res.books.length;
			} else {
				const res = await listBooks({
					sort,
					limit,
					offset: 0,
					author: selectedAuthor || undefined,
					tag: selectedTag || undefined,
					format: selectedFormat || undefined
				});
				books = res.books;
				total = res.total;
			}
		} catch (err) {
			console.error('Failed to load books:', err);
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		if (loadingMore || !hasMore) return;
		loadingMore = true;
		const nextOffset = books.length;
		try {
			const res = await listBooks({
				sort,
				limit,
				offset: nextOffset,
				author: selectedAuthor || undefined,
				tag: selectedTag || undefined,
				format: selectedFormat || undefined
			});
			books = [...books, ...res.books];
			total = res.total;
		} catch (err) {
			console.error('Failed to load more books:', err);
		} finally {
			loadingMore = false;
		}
	}

	function handleSearch() {
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(loadBooks, 300);
	}

	function handleSort() {
		loadBooks();
	}

	function handleFilter() {
		loadBooks();
	}

	function clearFilters() {
		selectedAuthor = '';
		selectedTag = '';
		selectedFormat = '';
		loadBooks();
	}

	function selectBook(id: number) {
		selectedBookId = id;
	}

	function closeDetail() {
		selectedBookId = null;
	}
</script>

<div class="library">
	<div class="controls">
		<input
			type="text"
			placeholder="search books..."
			bind:value={query}
			oninput={handleSearch}
			class="search"
		/>
		<select bind:value={sort} onchange={handleSort}>
			<option value="added">recently added</option>
			<option value="title">title</option>
			<option value="author">author</option>
		</select>
		<button
			class="filter-toggle"
			class:active={filtersOpen || hasActiveFilter}
			onclick={() => (filtersOpen = !filtersOpen)}
		>
			filter{hasActiveFilter ? ' *' : ''}
		</button>
	</div>

	{#if filtersOpen}
		<div class="filters">
			{#if authors.length > 0}
				<select bind:value={selectedAuthor} onchange={handleFilter}>
					<option value="">all authors</option>
					{#each authors as a}
						<option value={a.name}>{a.name} ({a.book_count})</option>
					{/each}
				</select>
			{/if}
			{#if tags.length > 0}
				<select bind:value={selectedTag} onchange={handleFilter}>
					<option value="">all tags</option>
					{#each tags as t}
						<option value={t.name}>{t.name} ({t.book_count})</option>
					{/each}
				</select>
			{/if}
			{#if formats.length > 0}
				<select bind:value={selectedFormat} onchange={handleFilter}>
					<option value="">all formats</option>
					{#each formats as f}
						<option value={f.name}>{f.name} ({f.book_count})</option>
					{/each}
				</select>
			{/if}
			{#if hasActiveFilter}
				<button class="clear-btn" onclick={clearFilters}>clear</button>
			{/if}
		</div>
	{/if}

	{#if loading}
		<div class="grid">
			{#each Array(12) as _}
				<BookCardSkeleton />
			{/each}
		</div>
	{:else if books.length === 0}
		<p class="status">no books found</p>
	{:else}
		<div class="grid">
			{#each books as book (book.id)}
				<BookCard {book} onselect={() => selectBook(book.id)} />
			{/each}
		</div>

		{#if loadingMore}
			<div class="grid">
				{#each Array(6) as _}
					<BookCardSkeleton />
				{/each}
			</div>
		{/if}

		<div bind:this={sentinel} class="sentinel"></div>
	{/if}
</div>

{#if selectedBookId !== null}
	<BookDetail bookId={selectedBookId} onclose={closeDetail} />
{/if}

<style>
	.library {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.controls {
		display: flex;
		gap: 0.75rem;
	}

	.search {
		flex: 1;
	}

	select {
		width: auto;
		min-width: 10rem;
	}

	.filter-toggle {
		font-size: 0.8rem;
		padding: 0.5rem 0.75rem;
		background: transparent;
		color: var(--fg-muted);
		border: 1px solid var(--border);
	}

	.filter-toggle:hover,
	.filter-toggle.active {
		color: var(--fg);
		border-color: var(--fg);
	}

	.filters {
		display: flex;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.filters select {
		min-width: 12rem;
	}

	.clear-btn {
		font-size: 0.75rem;
		padding: 0.5rem 0.75rem;
		background: transparent;
		color: var(--fg-muted);
		border: 1px solid var(--border);
	}

	.clear-btn:hover {
		color: var(--danger);
		border-color: var(--danger);
	}

	.status {
		color: var(--fg-muted);
		font-size: 0.85rem;
		padding: 3rem 0;
		text-align: center;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
		gap: 1.5rem;
	}

	.sentinel {
		height: 1px;
	}

	@media (max-width: 640px) {
		.grid {
			grid-template-columns: repeat(2, 1fr);
			gap: 1rem;
		}

		.controls {
			flex-direction: column;
		}

		.filters {
			flex-direction: column;
		}

		select, .filters select {
			width: 100%;
			min-width: 0;
		}
	}
</style>
