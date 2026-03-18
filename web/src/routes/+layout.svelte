<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { initAuth, getAuth } from '$lib/auth.svelte';
	import { checkSetup } from '$lib/api';
	import ShelfRat from '$lib/ShelfRat.svelte';

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

</script>

{#if ready || ['/login', '/setup'].some((p) => page.url.pathname.startsWith(p)) || page.url.pathname.startsWith('/invite')}
	{#if auth.isLoggedIn && !page.url.pathname.startsWith('/login') && !page.url.pathname.startsWith('/setup') && !page.url.pathname.startsWith('/invite')}
		<header>
			<div class="header-inner">
				<a href="/" class="logo"><ShelfRat /></a>
				<nav>
					<a href="/" class:active={page.url.pathname === '/'}>library</a>
					{#if auth.isAdmin}
						<a href="/admin" class:active={page.url.pathname.startsWith('/admin')}>admin</a>
					{/if}
					<a href="/profile" class="profile-link" class:active={page.url.pathname === '/profile'}>
						<svg class="profile-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
							<circle cx="8" cy="5.5" r="3" />
							<path d="M2 14.5c0-3 2.7-5 6-5s6 2 6 5" />
						</svg>
						{auth.user?.display_name || auth.user?.username || 'profile'}
					</a>
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

	main {
		max-width: var(--max-w);
		margin: 0 auto;
		padding: 2rem 1.5rem;
	}

	.profile-link {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.profile-icon {
		width: 1rem;
		height: 1rem;
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

		main {
			padding: 1.25rem 1rem;
		}
	}
</style>
