<script lang="ts">
	import { onMount } from 'svelte';
	import { getAuditLog } from '$lib/api';

	let entries: any[] = $state([]);
	let total = $state(0);
	let loading = $state(true);
	let actionFilter = $state('');
	let offset = $state(0);
	const limit = 50;

	onMount(() => {
		loadLog();
	});

	async function loadLog() {
		loading = true;
		try {
			const res = await getAuditLog({
				action: actionFilter || undefined,
				limit,
				offset
			});
			entries = res.entries;
			total = res.total;
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	function handleFilter() {
		offset = 0;
		loadLog();
	}

	function formatTime(d: string): string {
		return new Date(d).toLocaleString();
	}
</script>

<div class="controls">
	<select bind:value={actionFilter} onchange={handleFilter}>
		<option value="">all actions</option>
		<option value="login">login</option>
		<option value="book_sent">book sent</option>
		<option value="invite_created">invite created</option>
		<option value="user_joined">user joined</option>
		<option value="user_revoked">user revoked</option>
		<option value="profile_updated">profile updated</option>
		<option value="settings_updated">settings updated</option>
	</select>
</div>

{#if loading}
	<p class="status">loading...</p>
{:else if entries.length === 0}
	<p class="status">no entries</p>
{:else}
	<div class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>time</th>
				<th>user</th>
				<th>action</th>
				<th>detail</th>
			</tr>
		</thead>
		<tbody>
			{#each entries as entry (entry.id)}
				<tr>
					<td class="time">{formatTime(entry.created_at)}</td>
					<td>{entry.username || '—'}</td>
					<td><span class="badge">{entry.action}</span></td>
					<td class="detail">{entry.detail || ''}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	</div>

	{#if total > limit}
		<div class="pagination">
			<button class="secondary" onclick={() => { offset = Math.max(0, offset - limit); loadLog(); }} disabled={offset === 0}>prev</button>
			<span class="page-info">{offset + 1}–{Math.min(offset + limit, total)} of {total}</span>
			<button class="secondary" onclick={() => { offset += limit; loadLog(); }} disabled={offset + limit >= total}>next</button>
		</div>
	{/if}
{/if}

<style>
	.controls {
		display: flex;
		gap: 0.5rem;
	}

	.controls select {
		width: auto;
		min-width: 12rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8rem;
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
		border-bottom: 1px solid var(--border);
		padding: 0.5rem 0.75rem;
	}

	td {
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--border);
		vertical-align: top;
	}

	.time {
		white-space: nowrap;
		font-size: 0.75rem;
		color: var(--fg-muted);
	}

	.detail {
		font-size: 0.75rem;
		color: var(--fg-muted);
		max-width: 24rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
	}

	.page-info {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	@media (max-width: 640px) {
		table {
			min-width: 36rem;
		}

		.controls select {
			width: 100%;
			min-width: 0;
		}
	}
</style>
