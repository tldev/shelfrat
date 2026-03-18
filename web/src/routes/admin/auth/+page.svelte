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
	let copiedCallback = $state(false);
	let copiedScopes = $state(false);

	let scopes = $derived(
		settings.oidc_admin_value ? 'openid email profile groups' : 'openid email profile'
	);

	let callbackUrl = $derived(
		settings.app_url
			? `${settings.app_url.replace(/\/+$/, '')}/api/v1/auth/oidc/callback`
			: ''
	);

	function copyText(text: string, which: 'callback' | 'scopes') {
		navigator.clipboard.writeText(text);
		if (which === 'callback') {
			copiedCallback = true;
			setTimeout(() => (copiedCallback = false), 2000);
		} else {
			copiedScopes = true;
			setTimeout(() => (copiedScopes = false), 2000);
		}
	}

	onMount(() => {
		loadData();
	});

	async function loadData() {
		loading = true;
		try {
			const res = await getSettings();
			settings = res.settings;
			envLocked = res.env_locked;
			if (!settings.app_url) {
				settings.app_url = window.location.origin;
			}
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
			const oidcSettings: Record<string, string> = {
				app_url: settings.app_url || '',
				oidc_provider_name: settings.oidc_provider_name || '',
				oidc_issuer_url: settings.oidc_issuer_url || '',
				oidc_client_id: settings.oidc_client_id || '',
				oidc_auto_register: settings.oidc_auto_register || 'true',
				oidc_admin_claim: settings.oidc_admin_claim || '',
				oidc_admin_value: settings.oidc_admin_value || '',
			};
			if (settings.oidc_client_secret) {
				oidcSettings.oidc_client_secret = settings.oidc_client_secret;
			}
			const res = await updateSettings(oidcSettings);
			message = `updated: ${res.updated.join(', ')}`;
			settings.oidc_client_secret = '';
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
		<h2>OIDC</h2>
		<div class="oidc-layout">
			<form class="settings-form" onsubmit={handleSave}>
				<LockedField key="app_url" label="app URL" type="url" placeholder="https://shelf.example.com" hint="public URL of this app (used for OIDC redirect)" bind:value={settings.app_url} {envLocked} />
				<LockedField key="oidc_provider_name" label="provider name" placeholder="e.g. Authentik, Keycloak" hint='shown on the login button as "sign in with [name]"' bind:value={settings.oidc_provider_name} {envLocked} />
				<LockedField key="oidc_issuer_url" label="issuer URL" type="url" placeholder="https://auth.example.com/realms/main" hint="OIDC provider discovery endpoint base" bind:value={settings.oidc_issuer_url} {envLocked} />
				<LockedField key="oidc_client_id" label="client ID" bind:value={settings.oidc_client_id} {envLocked} />
				<LockedField key="oidc_client_secret" label="client secret" type="password" placeholder="••••••••" bind:value={settings.oidc_client_secret} {envLocked} />
				<LockedField key="oidc_auto_register" label="auto-register new users" hint="create accounts automatically on first OIDC login" bind:value={settings.oidc_auto_register} {envLocked}
					options={[
						{ value: 'true', label: 'yes' },
						{ value: 'false', label: 'no' },
					]}
				/>
				<div class="field-group">
					<h3>role mapping</h3>
					<span class="hint">grant admin based on an OIDC claim. role is synced on every login.</span>
					<LockedField key="oidc_admin_claim" label="claim name" placeholder="groups" bind:value={settings.oidc_admin_claim} {envLocked} />
					<LockedField key="oidc_admin_value" label="admin value" placeholder="shelfrat-admin" bind:value={settings.oidc_admin_value} {envLocked} />
					{#if !envLocked.includes('oidc_admin_value')}
						<span class="hint">leave admin value blank to disable role mapping</span>
					{/if}
				</div>
				{#if message}
					<p class="result">{message}</p>
				{/if}
				<button type="submit" disabled={saving}>
					{saving ? 'saving...' : 'save settings'}
				</button>
			</form>

			<InfoBox title="provider setup">
				<p>When configuring your OIDC provider, use the following callback URL:</p>
				{#if callbackUrl}
					<div class="copyable-row">
						<code class="copyable-value">{callbackUrl}</code>
						<button class="secondary small" onclick={() => copyText(callbackUrl, 'callback')}>
							{copiedCallback ? 'copied' : 'copy'}
						</button>
					</div>
				{:else}
					<p class="hint">enter an app URL to generate the callback URL</p>
				{/if}
				<dl>
					<dt>scopes</dt>
					<dd>
						<div class="copyable-row">
							<code class="copyable-value">{scopes}</code>
							<button class="secondary small" onclick={() => copyText(scopes, 'scopes')}>
								{copiedScopes ? 'copied' : 'copy'}
							</button>
						</div>
					</dd>
					<dt>grant type</dt>
					<dd><code>authorization code</code></dd>
				</dl>
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

	.oidc-layout {
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

	.copyable-row {
		display: flex;
		gap: 0.5rem;
		align-items: center;
	}

	.copyable-value {
		flex: 1;
		font-size: 0.7rem;
		word-break: break-all;
		padding: 0.35rem 0.5rem;
		background: var(--bg-offset, rgba(128, 128, 128, 0.08));
		border: 1px solid var(--border);
	}

	.small {
		font-size: 0.7rem;
		padding: 0.2rem 0.5rem;
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		border-top: 1px solid var(--border);
		padding-top: 0.75rem;
	}

	.field-group h3 {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
		margin: 0;
	}

	.field-row {
		display: flex;
		gap: 0.75rem;
	}

	@media (max-width: 640px) {
		.oidc-layout {
			flex-direction: column;
			gap: 1.5rem;
		}

		.settings-form {
			max-width: none;
		}
	}
</style>
