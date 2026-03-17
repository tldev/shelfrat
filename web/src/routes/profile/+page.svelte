<script lang="ts">
	import { getAuth, setAuth } from '$lib/auth.svelte';
	import { updateUser } from '$lib/api';

	const auth = getAuth();

	let displayName = $state(auth.user?.display_name || '');
	let email = $state(auth.user?.email || '');
	let kindleEmail = $state(auth.user?.kindle_email || '');
	let currentPassword = $state('');
	let newPassword = $state('');
	let saving = $state(false);
	let message = $state('');
	let error = $state('');

	async function handleSave(e: Event) {
		e.preventDefault();
		if (!auth.user) return;

		saving = true;
		error = '';
		message = '';

		try {
			const data: Record<string, string> = {};
			if (displayName !== (auth.user.display_name || '')) data.display_name = displayName;
			if (email !== auth.user.email) data.email = email;
			if (kindleEmail !== (auth.user.kindle_email || '')) data.kindle_email = kindleEmail;
			if (newPassword) {
				data.current_password = currentPassword;
				data.new_password = newPassword;
			}

			if (Object.keys(data).length === 0) {
				message = 'no changes';
				return;
			}

			const updated = await updateUser(auth.user.id, data);
			setAuth(localStorage.getItem('token')!, updated);
			message = 'profile updated';
			currentPassword = '';
			newPassword = '';
		} catch (err: any) {
			error = err.message || 'Failed to update';
		} finally {
			saving = false;
		}
	}
</script>

<h1>profile</h1>

<form class="profile-form" onsubmit={handleSave}>
	<div class="field">
		<label for="username">username</label>
		<input id="username" type="text" value={auth.user?.username || ''} disabled />
	</div>

	<div class="field">
		<label for="display_name">display name</label>
		<input id="display_name" type="text" bind:value={displayName} />
	</div>

	<div class="field">
		<label for="email">email</label>
		<input id="email" type="email" bind:value={email} />
	</div>

	<div class="field">
		<label for="kindle_email">kindle email</label>
		<input id="kindle_email" type="email" bind:value={kindleEmail} placeholder="your-kindle@kindle.com" />
	</div>

	<hr />

	<div class="field">
		<label for="current_password">current password (required to change password)</label>
		<input id="current_password" type="password" bind:value={currentPassword} autocomplete="current-password" />
	</div>

	<div class="field">
		<label for="new_password">new password</label>
		<input id="new_password" type="password" bind:value={newPassword} autocomplete="new-password" />
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}
	{#if message}
		<p class="message">{message}</p>
	{/if}

	<button type="submit" disabled={saving}>
		{saving ? 'saving...' : 'save changes'}
	</button>
</form>

<style>
	h1 {
		margin-bottom: 1.5rem;
	}

	.profile-form {
		max-width: var(--max-w-narrow);
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.field {
		display: flex;
		flex-direction: column;
	}

	hr {
		border: none;
		border-top: 1px solid var(--border);
		margin: 0.5rem 0;
	}

	.message {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
