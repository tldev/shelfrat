<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { initAuth, getAuth, clearAuth } from '$lib/auth.svelte';
	import { checkSetup } from '$lib/api';
	import ShelfRat from '$lib/ShelfRat.svelte';
	import ThemeToggle from '$lib/ThemeToggle.svelte';

	let { children } = $props();
	let ready = $state(false);
	const auth = getAuth();

	onMount(async () => {
		initAuth();

		const publicPaths = ['/login', '/setup', '/invite'];
		const isPublic = publicPaths.some((p) => page.url.pathname.startsWith(p));

		if (!isPublic && !auth.isLoggedIn) {
			try {
				const status = await checkSetup();
				if (!status.setup_complete) {
					goto('/setup');
					return;
				}
			} catch {}
			goto('/login');
			return;
		}

		ready = true;
	});

	function logout() {
		clearAuth();
		goto('/login');
	}
</script>

{#if ready || ['/login', '/setup'].some((p) => page.url.pathname.startsWith(p)) || page.url.pathname.startsWith('/invite')}
	{#if auth.isLoggedIn && !page.url.pathname.startsWith('/login') && !page.url.pathname.startsWith('/setup') && !page.url.pathname.startsWith('/invite')}
		<header>
			<div class="header-inner">
				<a href="/" class="logo"><ShelfRat /></a>
				<nav>
					<a href="/" class:active={page.url.pathname === '/'}>library</a>
					<a href="/profile" class:active={page.url.pathname === '/profile'}>profile</a>
					{#if auth.isAdmin}
						<a href="/admin" class:active={page.url.pathname.startsWith('/admin')}>admin</a>
					{/if}
					<button class="nav-btn" onclick={logout}>logout</button>
				<ThemeToggle />
				</nav>
			</div>
		</header>
		<main>
			{@render children()}
		</main>
	{:else}
		{@render children()}
	{/if}
{/if}

<style>
	header {
		border-bottom: 1px solid var(--border);
	}

	.header-inner {
		max-width: var(--max-w);
		margin: 0 auto;
		padding: 0.75rem 1.5rem;
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.logo {
		font-weight: 500;
		font-size: 0.9rem;
		color: var(--fg);
		text-decoration: none;
	}

	nav {
		display: flex;
		align-items: center;
		gap: 1.5rem;
	}

	nav a {
		font-size: 0.8rem;
		color: var(--fg-muted);
		text-decoration: none;
	}

	nav a:hover,
	nav a.active {
		color: var(--fg);
	}

	.nav-btn {
		font-size: 0.8rem;
		padding: 0.25rem 0.5rem;
		background: transparent;
		color: var(--fg-muted);
		border: none;
	}

	.nav-btn:hover {
		color: var(--fg);
	}

	main {
		max-width: var(--max-w);
		margin: 0 auto;
		padding: 2rem 1.5rem;
	}

	@media (max-width: 640px) {
		.header-inner {
			padding: 0.6rem 1rem;
		}

		nav {
			gap: 1rem;
		}

		nav a {
			font-size: 0.75rem;
		}

		.nav-btn {
			font-size: 0.75rem;
		}

		main {
			padding: 1.25rem 1rem;
		}
	}
</style>
