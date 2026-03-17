<script lang="ts">
	import { goto } from '$app/navigation';
	import { login as apiLogin, checkSetup, getOidcStatus, getOidcAuthorize, getMe } from '$lib/api';
	import { setAuth, initAuth, getAuth } from '$lib/auth.svelte';
	import { onMount } from 'svelte';
	import ShelfRat from '$lib/ShelfRat.svelte';

	let username = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);
	let oidcEnabled = $state(false);
	let oidcLoading = $state(false);

	onMount(async () => {
		// Check for OIDC callback token in URL fragment
		const hash = window.location.hash;
		if (hash.startsWith('#oidc_token=')) {
			const token = hash.slice('#oidc_token='.length);
			history.replaceState(null, '', '/login');
			try {
				localStorage.setItem('token', token);
				const user = await getMe();
				setAuth(token, user);
				goto('/');
				return;
			} catch {
				localStorage.removeItem('token');
				error = 'SSO login failed';
			}
		}

		// Check for OIDC error in query params
		const params = new URLSearchParams(window.location.search);
		const oidcError = params.get('error');
		if (oidcError === 'oidc_failed') {
			error = 'SSO login failed';
			history.replaceState(null, '', '/login');
		} else if (oidcError === 'oidc_no_account') {
			error = 'No account found. Contact your admin for access.';
			history.replaceState(null, '', '/login');
		}

		initAuth();
		const auth = getAuth();
		if (auth.isLoggedIn) {
			goto('/');
			return;
		}
		try {
			const status = await checkSetup();
			if (!status.setup_complete) {
				goto('/setup');
				return;
			}
		} catch {}

		try {
			const oidcStatus = await getOidcStatus();
			oidcEnabled = oidcStatus.enabled;
		} catch {}
	});

	async function handleLogin(e: Event) {
		e.preventDefault();
		error = '';
		loading = true;
		try {
			const res = await apiLogin(username, password);
			setAuth(res.token, res.user);
			goto('/');
		} catch (err: any) {
			error = err.message || 'Login failed';
		} finally {
			loading = false;
		}
	}

	async function handleOidc() {
		oidcLoading = true;
		error = '';
		try {
			const res = await getOidcAuthorize();
			window.location.href = res.url;
		} catch (err: any) {
			error = err.message || 'SSO login failed';
			oidcLoading = false;
		}
	}
</script>

<div class="page">
	<div class="form-container">
		<h1><ShelfRat /></h1>
		<p class="subtitle">sign in to your library</p>

		<form onsubmit={handleLogin}>
			<div class="field">
				<label for="username">username</label>
				<input id="username" type="text" bind:value={username} required autocomplete="username" />
			</div>
			<div class="field">
				<label for="password">password</label>
				<input id="password" type="password" bind:value={password} required autocomplete="current-password" />
			</div>
			{#if error}
				<p class="error">{error}</p>
			{/if}
			<button type="submit" disabled={loading}>
				{loading ? 'signing in...' : 'sign in'}
			</button>
		</form>

		{#if oidcEnabled}
			<div class="divider"><span>or</span></div>
			<button class="secondary oidc-btn" onclick={handleOidc} disabled={oidcLoading}>
				{oidcLoading ? 'redirecting...' : 'sign in with SSO'}
			</button>
		{/if}
	</div>
</div>

<style>
	.page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
	}

	.form-container {
		width: 100%;
		max-width: var(--max-w-narrow);
	}

	h1 {
		margin-bottom: 0.25rem;
	}

	.subtitle {
		color: var(--fg-muted);
		font-size: 0.85rem;
		margin-bottom: 2rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.field {
		display: flex;
		flex-direction: column;
	}

	button {
		margin-top: 0.5rem;
	}

	.divider {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin: 1.25rem 0;
		color: var(--fg-muted);
		font-size: 0.8rem;
	}

	.divider::before,
	.divider::after {
		content: '';
		flex: 1;
		border-top: 1px solid var(--border);
	}

	.oidc-btn {
		width: 100%;
		margin-top: 0;
	}
</style>
