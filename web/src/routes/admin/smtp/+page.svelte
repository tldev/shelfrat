<script lang="ts">
	import { onMount } from 'svelte';
	import { getSettings, updateSettings } from '$lib/api';
	import InfoBox from '$lib/InfoBox.svelte';
	import LockedField from '$lib/LockedField.svelte';

	let settings: Record<string, string> = $state({});
	let envLocked: string[] = $state([]);
	let loading = $state(true);
	let saving = $state(false);
	let message = $state('');

	onMount(() => {
		loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const res = await getSettings();
			settings = res.settings;
			envLocked = res.env_locked;
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

{#if loading}
	<p class="status">loading...</p>
{:else}
	<section>
		<h2>smtp settings</h2>
		<div class="smtp-layout">
			<form class="settings-form" onsubmit={handleSave}>
				<LockedField key="smtp_host" label="smtp host" placeholder="smtp.gmail.com" bind:value={settings.smtp_host} {envLocked} />
				<LockedField key="smtp_port" label="smtp port" placeholder="587" bind:value={settings.smtp_port} {envLocked} />
				<LockedField key="smtp_user" label="smtp user" bind:value={settings.smtp_user} {envLocked} />
				<LockedField key="smtp_password" label="smtp password" type="password" placeholder="••••••••" bind:value={settings.smtp_password} {envLocked} />
				<LockedField key="smtp_from" label="from email" type="email" bind:value={settings.smtp_from} {envLocked} />
				<LockedField key="smtp_encryption" label="encryption" bind:value={settings.smtp_encryption} {envLocked}
					options={[
						{ value: 'tls', label: 'TLS' },
						{ value: 'starttls', label: 'STARTTLS' },
						{ value: 'none', label: 'None' },
					]}
				/>
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

<style>
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
