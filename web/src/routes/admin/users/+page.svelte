<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { getAuth } from '$lib/auth.svelte';
	import { listUsers, createInvite, revokeUser, updateUser, type User } from '$lib/api';

	const auth = getAuth();

	let users: User[] = $state([]);
	let loading = $state(true);
	let inviteUrl = $state('');
	let message = $state('');

	onMount(async () => {
		if (!auth.isAdmin) {
			goto('/');
			return;
		}
		await loadUsers();
	});

	async function loadUsers() {
		loading = true;
		try {
			const res = await listUsers();
			users = res.users;
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	async function handleInvite() {
		try {
			const res = await createInvite();
			inviteUrl = `${window.location.origin}/invite/${res.invite_token}`;
		} catch (err: any) {
			message = err.message || 'Failed to create invite';
		}
	}

	async function handleRevoke(user: User) {
		if (!confirm(`Revoke access for ${user.username}?`)) return;
		try {
			await revokeUser(user.id);
			message = `${user.username} revoked`;
			await loadUsers();
		} catch (err: any) {
			message = err.message || 'Failed to revoke';
		}
	}

	async function handleToggleRole(user: User) {
		const newRole = user.role === 'admin' ? 'member' : 'admin';
		const action = newRole === 'admin' ? 'Promote' : 'Demote';
		if (!confirm(`${action} ${user.username} to ${newRole}?`)) return;
		try {
			await updateUser(user.id, { role: newRole });
			message = `${user.username} is now ${newRole}`;
			await loadUsers();
		} catch (err: any) {
			message = err.message || 'Failed to update role';
		}
	}

	function copyInvite() {
		navigator.clipboard.writeText(inviteUrl);
		message = 'copied to clipboard';
	}

	function formatDate(d: string | undefined): string {
		if (!d) return '';
		return new Date(d).toLocaleDateString();
	}
</script>

<div class="admin">
	<h1>admin</h1>

	<nav class="admin-nav">
		<a href="/admin">settings</a>
		<a href="/admin/users" class="active">users</a>
		<a href="/admin/auth">auth</a>
		<a href="/admin/providers">providers</a>
		<a href="/admin/smtp">smtp</a>
		<a href="/admin/jobs">jobs</a>
		<a href="/admin/audit">audit log</a>
	</nav>

	<div class="invite-section">
		<button onclick={handleInvite}>generate invite link</button>
		{#if inviteUrl}
			<div class="invite-url">
				<input type="text" value={inviteUrl} readonly />
				<button class="secondary" onclick={copyInvite}>copy</button>
			</div>
		{/if}
	</div>

	{#if message}
		<p class="result">{message}</p>
	{/if}

	{#if loading}
		<p class="status">loading...</p>
	{:else}
		<div class="table-wrap">
		<table>
			<thead>
				<tr>
					<th>username</th>
					<th>email</th>
					<th>role</th>
					<th>joined</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each users as user (user.id)}
					<tr>
						<td>{user.username}</td>
						<td>{user.email || '—'}</td>
						<td><span class="badge">{user.role}</span></td>
						<td>{formatDate(user.created_at)}</td>
						<td class="actions">
							{#if user.id !== auth.user?.id}
								<button class="small" onclick={() => handleToggleRole(user)}>
									{user.role === 'admin' ? 'demote' : 'promote'}
								</button>
								<button class="danger small" onclick={() => handleRevoke(user)}>revoke</button>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
		</div>
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

	.invite-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: 36rem;
	}

	.invite-url {
		display: flex;
		gap: 0.5rem;
	}

	.invite-url input {
		flex: 1;
		font-size: 0.75rem;
	}

	.result {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.status {
		color: var(--fg-muted);
		font-size: 0.85rem;
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
	}

	.actions {
		white-space: nowrap;
		text-align: right;
	}

	.actions :global(button + button) {
		margin-left: 0.35rem;
	}

	.small {
		font-size: 0.7rem;
		padding: 0.2rem 0.5rem;
	}

	@media (max-width: 640px) {
		table {
			min-width: 32rem;
		}

		.invite-section {
			max-width: none;
		}
	}
</style>
