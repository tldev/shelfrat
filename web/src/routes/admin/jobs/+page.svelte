<script lang="ts">
	import { onMount } from 'svelte';
	import { getJobs, triggerJob, getJobRuns, updateJobCadence } from '$lib/api';

	let jobs: any[] = $state([]);
	let loading = $state(true);
	let runningJobs: Record<string, boolean> = $state({});
	let runMessages: Record<string, string> = $state({});
	let cadenceInputs: Record<string, number> = $state({});
	let savingCadence: Record<string, boolean> = $state({});
	let cadenceMessages: Record<string, string> = $state({});
	let expandedRuns: Record<string, boolean> = $state({});
	let jobRuns: Record<string, any[]> = $state({});
	let loadingRuns: Record<string, boolean> = $state({});
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	onMount(() => {
		loadJobs();
		return () => {
			if (pollInterval) clearInterval(pollInterval);
		};
	});

	async function loadJobs() {
		try {
			const res = await getJobs();
			jobs = res.jobs;
			for (const job of jobs) {
				if (!(job.name in cadenceInputs)) {
					cadenceInputs[job.name] = job.cadence_seconds ?? 0;
				}
			}
			updatePolling();
		} catch (err) {
			console.error(err);
		} finally {
			loading = false;
		}
	}

	function updatePolling() {
		const anyRunning = jobs.some(j => j.status === 'running') || Object.values(runningJobs).some(Boolean);
		if (anyRunning && !pollInterval) {
			pollInterval = setInterval(() => loadJobs(), 5000);
		} else if (!anyRunning && pollInterval) {
			clearInterval(pollInterval);
			pollInterval = null;
		}
	}

	async function handleRun(name: string) {
		runningJobs[name] = true;
		runMessages[name] = '';
		try {
			await triggerJob(name);
			runMessages[name] = 'triggered';
			await loadJobs();
		} catch (err: any) {
			runMessages[name] = err.message || 'failed to trigger';
		} finally {
			runningJobs[name] = false;
		}
	}

	async function handleSaveCadence(name: string) {
		savingCadence[name] = true;
		cadenceMessages[name] = '';
		try {
			await updateJobCadence(name, cadenceInputs[name]);
			cadenceMessages[name] = 'saved';
			await loadJobs();
		} catch (err: any) {
			cadenceMessages[name] = err.message || 'failed to save';
		} finally {
			savingCadence[name] = false;
		}
	}

	async function toggleRuns(name: string) {
		expandedRuns[name] = !expandedRuns[name];
		if (expandedRuns[name] && !jobRuns[name]) {
			loadingRuns[name] = true;
			try {
				const res = await getJobRuns(name);
				jobRuns[name] = res.runs;
			} catch (err) {
				console.error(err);
			} finally {
				loadingRuns[name] = false;
			}
		}
	}

	function formatCadence(seconds: number): string {
		if (seconds <= 0) return 'disabled';
		if (seconds < 60) return `${seconds} second${seconds !== 1 ? 's' : ''}`;
		if (seconds < 3600) {
			const m = Math.floor(seconds / 60);
			return `${m} minute${m !== 1 ? 's' : ''}`;
		}
		if (seconds < 86400) {
			const h = Math.floor(seconds / 3600);
			return `${h} hour${h !== 1 ? 's' : ''}`;
		}
		const d = Math.floor(seconds / 86400);
		return `${d} day${d !== 1 ? 's' : ''}`;
	}

	function formatTime(d: string | null): string {
		if (!d) return '—';
		return new Date(d).toLocaleString();
	}

	function formatDuration(ms: number | null): string {
		if (ms == null) return '—';
		if (ms < 1000) return `${ms}ms`;
		const s = (ms / 1000).toFixed(1);
		return `${s}s`;
	}
</script>

{#if loading}
	<p class="status">loading...</p>
{:else if jobs.length === 0}
	<p class="status">no jobs configured</p>
{:else}
	{#each jobs as job (job.name)}
		<section class="job-card">
			<div class="job-header">
				<div>
					<h2>{job.name}</h2>
					{#if job.description}
						<p class="job-description">{job.description}</p>
					{/if}
				</div>
				<button onclick={() => handleRun(job.name)} disabled={runningJobs[job.name] || job.status === 'running'}>
					{runningJobs[job.name] || job.status === 'running' ? 'running...' : 'run now'}
				</button>
			</div>
			{#if runMessages[job.name]}
				<p class="result">{runMessages[job.name]}</p>
			{/if}

			<div class="cadence-row">
				<span class="cadence-label">every</span>
				<input
					id="cadence_{job.name}"
					type="number"
					min="0"
					bind:value={cadenceInputs[job.name]}
				/>
				<span class="cadence-label">s ({formatCadence(cadenceInputs[job.name] ?? 0)})</span>
				<button class="secondary small" onclick={() => handleSaveCadence(job.name)} disabled={savingCadence[job.name]}>
					{savingCadence[job.name] ? '...' : 'save'}
				</button>
				{#if cadenceMessages[job.name]}
					<span class="result">{cadenceMessages[job.name]}</span>
				{/if}
			</div>

			{#if job.last_run}
				<p class="last-run">
					<span class="badge">{job.last_run.status}</span>
					{formatTime(job.last_run.started_at)}
					{#if job.last_run.duration_ms != null}· {formatDuration(job.last_run.duration_ms)}{/if}
					{#if job.last_run.result_summary}· {job.last_run.result_summary}{/if}
					<button class="link" onclick={() => toggleRuns(job.name)}>
						{expandedRuns[job.name] ? 'hide history' : 'history'}
					</button>
				</p>
			{/if}

			{#if expandedRuns[job.name]}
				{#if loadingRuns[job.name]}
					<p class="status">loading...</p>
				{:else if jobRuns[job.name]?.length === 0}
					<p class="status">no runs yet</p>
				{:else if jobRuns[job.name]}
					<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>started</th>
								<th>status</th>
								<th>duration</th>
								<th>result</th>
							</tr>
						</thead>
						<tbody>
							{#each jobRuns[job.name] as run}
								<tr>
									<td class="time">{formatTime(run.started_at)}</td>
									<td><span class="badge">{run.status}</span></td>
									<td>{formatDuration(run.duration_ms)}</td>
									<td class="detail">{run.result_summary || '—'}</td>
								</tr>
							{/each}
						</tbody>
					</table>
					</div>
				{/if}
			{/if}
		</section>
	{/each}
{/if}

<style>
	.job-card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		border-bottom: 1px solid var(--border);
		padding-bottom: 1.25rem;
	}

	.job-header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 1rem;
	}

	.job-description {
		font-size: 0.8rem;
		color: var(--fg-muted);
		margin-top: 0.1rem;
	}

	.cadence-row {
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}

	.cadence-row input {
		width: 5rem;
	}

	.cadence-label {
		font-size: 0.8rem;
		color: var(--fg-muted);
		white-space: nowrap;
	}

	.last-run {
		font-size: 0.8rem;
		color: var(--fg-muted);
		display: flex;
		align-items: center;
		gap: 0.4rem;
		flex-wrap: wrap;
	}

	.small {
		font-size: 0.7rem;
		padding: 0.2rem 0.5rem;
	}

	.link {
		background: none;
		border: none;
		padding: 0;
		font-size: 0.75rem;
		color: var(--fg-muted);
		text-decoration: underline;
		cursor: pointer;
	}

	.link:hover {
		color: var(--fg);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8rem;
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
		border-bottom: 1px solid var(--border);
		padding: 0.5rem 0.75rem;
	}

	td {
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--border);
		vertical-align: top;
	}

	.time {
		white-space: nowrap;
		font-size: 0.75rem;
		color: var(--fg-muted);
	}

	.detail {
		font-size: 0.75rem;
		color: var(--fg-muted);
		max-width: 24rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 640px) {
		.job-header {
			flex-direction: column;
			gap: 0.5rem;
		}

		.cadence-row {
			flex-wrap: wrap;
		}

		table {
			min-width: 32rem;
		}
	}
</style>
