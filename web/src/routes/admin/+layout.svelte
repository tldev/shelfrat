<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getAuth } from '$lib/auth.svelte';

	let { children } = $props();
	const auth = getAuth();
	let ready = $state(false);

	const tabs = [
		{ href: '/admin', label: 'library & metadata', exact: true },
		{ href: '/admin/users', label: 'users' },
		{ href: '/admin/auth', label: 'oidc' },
		{ href: '/admin/smtp', label: 'smtp' },
		{ href: '/admin/jobs', label: 'jobs' },
		{ href: '/admin/audit', label: 'audit log' },
	];

	onMount(() => {
		if (!auth.isAdmin) {
			goto('/');
			return;
		}
		ready = true;
	});
</script>

{#if ready}
	<div class="admin">
		<h1>admin</h1>
		<nav class="admin-nav">
			{#each tabs as tab}
				<a
					href={tab.href}
					class:active={tab.exact ? page.url.pathname === tab.href : page.url.pathname.startsWith(tab.href)}
				>{tab.label}</a>
			{/each}
		</nav>
		{@render children()}
	</div>
{/if}

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

	.admin :global(.result) {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.admin :global(.status) {
		color: var(--fg-muted);
		font-size: 0.85rem;
	}

	.admin :global(.hint) {
		font-size: 0.7rem;
		color: var(--fg-muted);
	}

	.admin :global(.field) {
		display: flex;
		flex-direction: column;
	}
</style>
