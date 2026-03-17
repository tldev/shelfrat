<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { registerWithInvite } from '$lib/api';
	import ShelfRat from '$lib/ShelfRat.svelte';

	let username = $state('');
	let email = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);

	const token = page.params.token!;

	async function handleRegister(e: Event) {
		e.preventDefault();
		error = '';
		loading = true;
		try {
			await registerWithInvite(token, { username, email, password });
			goto('/login');
		} catch (err: any) {
			error = err.message || 'Registration failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="page">
	<div class="form-container">
		<h1><ShelfRat /></h1>
		<p class="subtitle">you've been invited — create your account</p>

		<form onsubmit={handleRegister}>
			<div class="field">
				<label for="username">username</label>
				<input id="username" type="text" bind:value={username} required />
			</div>
			<div class="field">
				<label for="email">email</label>
				<input id="email" type="email" bind:value={email} required />
			</div>
			<div class="field">
				<label for="password">password</label>
				<input id="password" type="password" bind:value={password} required minlength="8" />
			</div>
			{#if error}
				<p class="error">{error}</p>
			{/if}
			<button type="submit" disabled={loading}>
				{loading ? 'creating...' : 'create account'}
			</button>
		</form>
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
</style>
