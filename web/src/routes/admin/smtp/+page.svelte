<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { getAuth } from '$lib/auth.svelte';
	import { getSettings, updateSettings } from '$lib/api';
	import InfoBox from '$lib/InfoBox.svelte';

	const auth = getAuth();

	let settings: Record<string, string> = $state({});
	let loading = $state(true);
	let saving = $state(false);
	let message = $state('');

	onMount(async () => {
		if (!auth.isAdmin) {
			goto('/');
			return;
		}
		await loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const res = await getSettings();
			settings = res.settings;
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	async function handleSave(e: Event) {
		e.preventDefault();
		saving = true;
		message = '';
		try {
			const res = await updateSettings(settings);
			message = `updated: ${res.updated.join(', ')}`;
		} catch (err: any) {
			message = err.message || 'Failed to save';
		} finally {
			saving = false;
		}
	}
</script>

<div class="admin">
	<h1>admin</h1>

	<nav class="admin-nav">
		<a href="/admin">settings</a>
		<a href="/admin/users">users</a>
		<a href="/admin/auth">auth</a>
		<a href="/admin/providers">providers</a>
		<a href="/admin/smtp" class="active">smtp</a>
		<a href="/admin/jobs">jobs</a>
		<a href="/admin/audit">audit log</a>
	</nav>

	{#if loading}
		<p class="status">loading...</p>
	{:else}
		<section>
			<h2>smtp settings</h2>
			<div class="smtp-layout">
				<form class="settings-form" onsubmit={handleSave}>
					<div class="field">
						<label for="smtp_host">smtp host</label>
						<input id="smtp_host" type="text" bind:value={settings.smtp_host} placeholder="smtp.gmail.com" />
					</div>
					<div class="field">
						<label for="smtp_port">smtp port</label>
						<input id="smtp_port" type="text" bind:value={settings.smtp_port} placeholder="587" />
					</div>
					<div class="field">
						<label for="smtp_user">smtp user</label>
						<input id="smtp_user" type="text" bind:value={settings.smtp_user} />
					</div>
					<div class="field">
						<label for="smtp_password">smtp password</label>
						<input id="smtp_password" type="password" bind:value={settings.smtp_password} placeholder="••••••••" />
					</div>
					<div class="field">
						<label for="smtp_from">from email</label>
						<input id="smtp_from" type="email" bind:value={settings.smtp_from} />
					</div>
					<div class="field">
						<label for="smtp_encryption">encryption</label>
						<select id="smtp_encryption" bind:value={settings.smtp_encryption}>
							<option value="tls">TLS</option>
							<option value="starttls">STARTTLS</option>
							<option value="none">None</option>
						</select>
					</div>
					{#if message}
						<p class="result">{message}</p>
					{/if}
					<button type="submit" disabled={saving}>
						{saving ? 'saving...' : 'save settings'}
					</button>
				</form>

				<InfoBox title="gmail setup">
					<p>To send from a personal Gmail account, use an app password instead of your regular password. You'll need 2-step verification enabled first.</p>
					<dl>
						<dt>host</dt>
						<dd><code>smtp.gmail.com</code></dd>
						<dt>port</dt>
						<dd><code>587</code></dd>
						<dt>encryption</dt>
						<dd><code>STARTTLS</code></dd>
						<dt>user</dt>
						<dd><code>you@gmail.com</code></dd>
						<dt>password</dt>
						<dd>your 16-character app password</dd>
					</dl>
					<p><a href="https://myaccount.google.com/apppasswords" target="_blank" rel="noopener">create an app password</a></p>
				</InfoBox>
			</div>
		</section>
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

	section {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.smtp-layout {
		display: flex;
		gap: 2.5rem;
		align-items: flex-start;
	}

	.settings-form {
		flex: 1;
		max-width: var(--max-w-narrow);
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.field {
		display: flex;
		flex-direction: column;
	}

	.result {
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.status {
		color: var(--fg-muted);
		font-size: 0.85rem;
	}

	@media (max-width: 640px) {
		.smtp-layout {
			flex-direction: column;
			gap: 1.5rem;
		}

		.settings-form {
			max-width: none;
		}
	}
</style>
