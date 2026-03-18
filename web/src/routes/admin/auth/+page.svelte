<script lang="ts">
	import { onMount } from 'svelte';
	import { getSettings, updateSettings } from '$lib/api';
	import InfoBox from '$lib/InfoBox.svelte';

	let settings: Record<string, string> = $state({});
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
				<div class="field">
					<label for="app_url">app URL</label>
					<input id="app_url" type="url" bind:value={settings.app_url} placeholder="https://shelf.example.com" />
					<span class="hint">public URL of this app (used for OIDC redirect)</span>
				</div>
				<div class="field">
					<label for="oidc_provider_name">provider name</label>
					<input id="oidc_provider_name" type="text" bind:value={settings.oidc_provider_name} placeholder="e.g. Authentik, Keycloak" />
					<span class="hint">shown on the login button as "sign in with [name]"</span>
				</div>
				<div class="field">
					<label for="oidc_issuer_url">issuer URL</label>
					<input id="oidc_issuer_url" type="url" bind:value={settings.oidc_issuer_url} placeholder="https://auth.example.com/realms/main" />
					<span class="hint">OIDC provider discovery endpoint base</span>
				</div>
				<div class="field">
					<label for="oidc_client_id">client ID</label>
					<input id="oidc_client_id" type="text" bind:value={settings.oidc_client_id} />
				</div>
				<div class="field">
					<label for="oidc_client_secret">client secret</label>
					<input id="oidc_client_secret" type="password" bind:value={settings.oidc_client_secret} placeholder="••••••••" />
				</div>
				<div class="field">
					<label for="oidc_auto_register">auto-register new users</label>
					<select id="oidc_auto_register" bind:value={settings.oidc_auto_register}>
						<option value="true">yes</option>
						<option value="false">no</option>
					</select>
					<span class="hint">create accounts automatically on first OIDC login</span>
				</div>
				<div class="field-group">
					<h3>role mapping</h3>
					<span class="hint">grant admin based on an OIDC claim. role is synced on every login.</span>
					<div class="field-row">
						<div class="field">
							<label for="oidc_admin_claim">claim name</label>
							<input id="oidc_admin_claim" type="text" bind:value={settings.oidc_admin_claim} placeholder="groups" />
						</div>
						<div class="field">
							<label for="oidc_admin_value">admin value</label>
							<input id="oidc_admin_value" type="text" bind:value={settings.oidc_admin_value} placeholder="shelfrat-admin" />
						</div>
					</div>
					<span class="hint">leave admin value blank to disable role mapping</span>
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

	.field-row .field {
		flex: 1;
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
